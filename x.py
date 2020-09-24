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
    c.append("AFL_SKIP_BIN_CHECK=1")
    c.append("AFL_DEBUG_CHILD_OUTPUT=1")
    c.append("AFL_CUSTOM_MUTATOR_LIBRARY=/btrfs-fuzz/libmutator.so")
    c.append("AFL_CUSTOM_MUTATOR_ONLY=1")
    # The custom mutator doesn't append or delete bytes. Trimming also messes
    # with deserializing input so, don't trim.
    c.append("AFL_DISABLE_TRIM=1")
    c.append("/usr/local/bin/afl-fuzz")
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
        "state_dir",
        type=str,
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
        help="Shared state directory between host and VM, mounted in VM at /state",
    )
    shell.set_defaults(func=cmd_shell)

    help = subparsers.add_parser("help", help="print help")
    help.set_defaults(func=lambda _: parser.print_help())

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    proj_dir = pathlib.Path(__file__).parent
    os.chdir(proj_dir)

    main()
