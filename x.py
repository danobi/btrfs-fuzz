#!/bin/python3

import argparse
import os
import pathlib
import shutil
import subprocess
import sys

import src.manager as manager

DOCKER_IMAGE_REMOTE = "dxuu/btrfs-fuzz"
DOCKER_IMAGE_LOCAL = "localhost/btrfs-fuzz"


def sh(cmd):
    try:
        subprocess.run(cmd, shell=True, check=True)
    except subprocess.CalledProcessError as e:
        sys.exit(1)


# Docker tends to freak out if a directory begins with `_`
def sanitize_docker_dir(dir):
    if dir[0] == "/":
        return dir
    else:
        return "./" + dir


def cmd_build(args):
    if args.buildah:
        tool = "buildah"
    else:
        tool = "podman"

    if args.remote:
        sh(f"{tool} pull {DOCKER_IMAGE_REMOTE}")
    else:
        if args.buildah:
            tool += " build-using-dockerfile"
        else:
            tool += " build"

        sh(f"{tool} -t btrfs-fuzz .")


def cmd_build_tar(args):
    # First build the latest image
    if not args.no_build:
        cmd_build(args)

    tmpname = "btrfs-fuzz-tmp"

    if args.zstd and not args.file.endswith(".tzst"):
        args.file = args.file + ".tzst"
    elif not args.file.endswith(".tar"):
        args.file = args.file + ".tar"

    c = ["podman export"]
    c.append("$(")
    c.append("podman create --name")
    c.append(tmpname)

    if args.remote:
        c.append(DOCKER_IMAGE_REMOTE)
    else:
        c.append(DOCKER_IMAGE_LOCAL)

    c.append("/bin/true")
    c.append(")")

    if args.zstd:
        c.append("|")
        c.append("zstd")

    # Both `podman export` and zstd take the `-o OUTPUT` flag
    c.append("-o")
    c.append(args.file)

    sh(" ".join(c))
    sh(f"podman rm {tmpname}")


def cmd_run(args):
    print("Starting btrfs-fuzz")

    if args.remote:
        img = DOCKER_IMAGE_REMOTE
    else:
        img = DOCKER_IMAGE_LOCAL

    nspawn = False
    if args.fs_dir:
        # Must be root to use systemd-nspawn
        if os.geteuid() != 0:
            print("--fs-dir requires root")
            return

        img = args.fs_dir
        nspawn = True

    state_dir = sanitize_docker_dir(args.state_dir)

    m = manager.Manager(img, state_dir, nspawn=nspawn, parallel=args.parallel)
    m.run()


def cmd_shell(args):
    c = ["podman run"]
    c.append("-it")
    c.append("--privileged")

    if args.state_dir:
        c.append(f"-v {sanitize_docker_dir(args.state_dir)}:/state")

    if args.remote:
        c.append(DOCKER_IMAGE_REMOTE)
    else:
        c.append(DOCKER_IMAGE_LOCAL)

    sh(" ".join(c))


def cmd_seed(args):
    if pathlib.Path(args.state_dir).exists():
        print(f"{args.state_dir} already exists, noop-ing")
        return

    pathlib.Path.mkdir(pathlib.Path(f"{args.state_dir}/input"), parents=True)
    pathlib.Path.mkdir(pathlib.Path(f"{args.state_dir}/output"))
    pathlib.Path.mkdir(pathlib.Path(f"{args.state_dir}/known_crashes"))

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

    # Copy files from checked in corpus over too
    corpus_files = os.listdir("./corpus")
    for f in corpus_files:
        assert f.endswith(".zst")
        raw_fname = f[:-4]
        raw_path = f"{args.state_dir}/input/{raw_fname}"
        assert raw_path.endswith(".raw")
        compressed_fname = raw_fname[:-4]
        compressed_path = f"{args.state_dir}/input/{compressed_fname}"

        sh(f"zstd -d ./corpus/{f} -o {raw_path}")
        sh(f"cargo run -p imgcompress compress -- {raw_path} {compressed_path}")
        sh(f"rm {raw_path}")

    # Write a readme to describe what each directory contains
    readme_path = pathlib.Path(f"{args.state_dir}/README")
    with open(readme_path, "w") as f:
        content = "This directory holds all the state for a fuzzing session.\n\n"
        content += "Each subdirectory contains as follows:\n\n"
        content += (
            "known_crashes: test cast images that are known to cause a kernel panic\n"
        )
        content += "input: afl++ input directory\n"
        content += "output: afl++ output directory\n"
        f.write(content)


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

    if args.remote:
        c.append(DOCKER_IMAGE_REMOTE)
    else:
        c.append(DOCKER_IMAGE_LOCAL)

    p = pexpect.spawn(" ".join(c), encoding="utf-8")
    p.expect("root@.*#")
    p.sendline('/bin/bash -c "echo core > /proc/sys/kernel/core_pattern"')

    c = []
    c.append("/btrfs-fuzz/runner")
    c.append(f"< /state/{image_fname}")

    p.expect("root@.*#")

    if args.exit:
        # Send child output to stdout
        p.logfile_read = sys.stdout
        p.sendline(" ".join(c))

        p.expect("root@.*#")

        # `C-a x` to exit qemu
        p.sendcontrol("a")
        p.send("x")
    else:
        p.sendline(" ".join(c))

        # Give control back to terminal
        p.interact()


def main():
    parser = argparse.ArgumentParser(
        prog="x", formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )
    parser.add_argument("--remote", action="store_true", help="Use remote docker image")
    parser.set_defaults(func=lambda _: parser.print_help())

    subparsers = parser.add_subparsers(help="subcommands")

    build = subparsers.add_parser("build", help="build btrfs-fuzz components")
    build.add_argument(
        "-b",
        "--buildah",
        action="store_true",
        help="Use buildah to build image",
    )
    build.set_defaults(func=cmd_build)

    build_tar = subparsers.add_parser(
        "build-tar", help="build btrfs-fuzz image into a tarball"
    )
    build_tar.add_argument(
        "file",
        type=str,
        help="Filename for output tarball",
    )
    build_tar.add_argument(
        "--zstd",
        action="store_true",
        help="zstd compress archive",
    )
    build_tar.add_argument(
        "--no-build",
        action="store_true",
        help="Do not rebuild image if stale",
    )
    build_tar.add_argument(
        "-b",
        "--buildah",
        action="store_true",
        help="Use buildah to build image",
    )
    build_tar.set_defaults(func=cmd_build_tar)

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
    run.add_argument(
        "-f",
        "--fs-dir",
        type=str,
        help="Use systemd-nspawn instead of docker to run container. "
        "The provided argument is the directory to use as filesystem "
        "root (typically generated by `build-tar`). Requires root. "
        "When active, --remote is ignored.",
    )
    run.add_argument(
        "--parallel",
        type=int,
        help="Fuzz in parallel on PARALLEL # of cpus. -1 means all cpus.",
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
