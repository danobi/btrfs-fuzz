#!/bin/python3

import argparse
import os
import pathlib
import subprocess
import sys


def sh(cmd):
    try:
        subprocess.run(cmd, shell=True, check=True)
    except subprocess.CalledProcessError as e:
        sys.exit(1)


def cmd_build(args):
    if args.local:
        sh("podman build -t btrfs-fuzz .")
    else:
        sh("podman pull dxuu/btrfs-fuzz")


def cmd_run(args):
    import pexpect

    print("Starting btrfs-fuzz")

    c = ["podman run"]
    c.append("-it")
    c.append("--privileged")
    c.append(f"-v {args.state_dir}:/state")

    if args.local:
        c.append("localhost/btrfs-fuzz")
    else:
        c.append("dxuu/btrfs-fuzz")

    p = pexpect.spawn(" ".join(c))
    p.expect("root@.*#")
    p.sendline('/bin/bash -c "echo core > /proc/sys/kernel/core_pattern"')

    c = []
    # We didn't build with the afl toolchain so our binary is not watermarked
    c.append("AFL_SKIP_BIN_CHECK=1")

    # Help debug crashes in our runner
    c.append("AFL_DEBUG_CHILD_OUTPUT=1")

    # Our custom mutator only fuzzes the FS metadata. Anything else is
    # ineffective
    c.append("AFL_CUSTOM_MUTATOR_LIBRARY=/btrfs-fuzz/libmutator.so")
    c.append("AFL_CUSTOM_MUTATOR_ONLY=1")

    # The custom mutator doesn't append or delete bytes. Trimming also messes
    # with deserializing input so, don't trim.
    c.append("AFL_DISABLE_TRIM=1")

    c.append("/usr/local/bin/afl-fuzz")
    c.append("-m 500")
    c.append("-i /state/input")
    c.append("-o /state/output")
    c.append("-- /btrfs-fuzz/runner")

    p.expect("root@.*#")
    p.sendline(" ".join(c))

    # Give control back to terminal
    p.interact()


def cmd_shell(args):
    c = ["podman run"]
    c.append("-it")
    c.append("--privileged")

    if args.state_dir:
        c.append(f"-v {args.state_dir}:/state")

    if args.local:
        c.append("localhost/btrfs-fuzz")
    else:
        c.append("dxuu/btrfs-fuzz")

    sh(" ".join(c))


def cmd_seed(args):
    if pathlib.Path(args.state_dir).exists():
        print(f"{args.state_dir} already exists, noop-ing")
        return

    pathlib.Path.mkdir(pathlib.Path(f"{args.state_dir}/input"), parents=True)
    pathlib.Path.mkdir(pathlib.Path(f"{args.state_dir}/output"))

    # Generate raw image
    image_path = pathlib.Path(f"{args.state_dir}/input/image")
    with open(image_path, "wb") as i:
        # 120 MB is just about the minimum size for a raw btrfs image
        i.truncate(120 << 20)

        sh(f"mkfs.btrfs {image_path}")

    # Compress raw image into a new file and then remove the raw image
    compressed_image_path = f"{args.state_dir}/input/img_compressed"
    sh(f"cargo run --bin imgcompress -- compress {image_path} {compressed_image_path}")
    sh(f"rm {image_path}")


def cmd_repro(args):
    import pexpect

    print(f"Reproducing {args.image}")

    # Share the entire directory containing the image under test
    image_dir = str(pathlib.Path(args.image).parent)
    if image_dir[0] != "/":
        # Necessary so docker doesn't freak out
        image_dir = "./" + image_dir

    image_fname = str(pathlib.Path(args.image).name)

    c = ["podman run"]
    c.append("-it")
    c.append("--privileged")
    c.append(f"-v {image_dir}:/state")

    if args.local:
        c.append("localhost/btrfs-fuzz")
    else:
        c.append("dxuu/btrfs-fuzz")

    p = pexpect.spawn(" ".join(c), encoding="utf-8")
    p.expect("root@.*#")
    p.sendline('/bin/bash -c "echo core > /proc/sys/kernel/core_pattern"')

    c = []
    c.append("/btrfs-fuzz/runner")
    c.append(f"< /state/{image_fname}")

    p.expect("root@.*#")

    # Send all child output to stdout. We have to open stdout in bytes mode
    # otherwise pexpect freaks out.
    stdout = os.fdopen(sys.stdout.fileno(), "wb")
    p.logfile_read = stdout

    p.sendline(" ".join(c))

    if args.exit:
        p.expect("root@.*#")

        # `C-a x` to exit qemu
        p.sendcontrol("a")
        p.send("x")
    else:
        # Give control back to terminal
        p.interact()


def main():
    parser = argparse.ArgumentParser(
        prog="x", formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )
    parser.add_argument("--local", action="store_true", help="Use local docker image")
    parser.set_defaults(func=lambda _: parser.print_help())

    subparsers = parser.add_subparsers(help="subcommands")

    build = subparsers.add_parser("build", help="build btrfs-fuzz components")
    build.set_defaults(func=cmd_build)

    run = subparsers.add_parser("run", help="run fuzzer")
    run.add_argument(
        "-s",
        "--state-dir",
        type=str,
        default="./_state",
        help="Shared state directory between host and VM, mounted in VM at "
        "/state. The directory must contain `input` and `output` "
        "subdirectories, with `input` containing initial test cases.",
    )
    run.set_defaults(func=cmd_run)

    shell = subparsers.add_parser("shell", help="start shell in VM")
    shell.add_argument(
        "-s",
        "--state-dir",
        type=str,
        default="./_state",
        help="Shared state directory between host and VM, mounted in VM at /state",
    )
    shell.set_defaults(func=cmd_shell)

    seed = subparsers.add_parser("seed", help="seed input corpus")
    seed.add_argument(
        "-s",
        "--state-dir",
        type=str,
        default="./_state",
        help="Shared state directory between host and VM",
    )
    seed.set_defaults(func=cmd_seed)

    repro = subparsers.add_parser("repro", help="reproduce a test case")
    repro.add_argument(
        "image",
        type=str,
        help="btrfs filesystem image to test against (must be imgcompress-compressed)",
    )
    repro.add_argument(
        "--exit",
        action="store_true",
        help="Exit VM after repro runs (useful for scripting)",
    )
    repro.set_defaults(func=cmd_repro)

    help = subparsers.add_parser("help", help="print help")
    help.set_defaults(func=lambda _: parser.print_help())

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    proj_dir = pathlib.Path(__file__).parent
    os.chdir(proj_dir)

    main()
