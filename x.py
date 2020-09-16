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
        sh('podman build -t btrfs-fuzz .')
    else:
        sh('podman pull dxuu/btrfs-fuzz')


def cmd_run(args):
    print("Starting btrfs-fuzz")
    print("TODO")


def cmd_shell(args):
    c = ['podman run']
    c.append('-it')
    c.append('--privileged')

    if args.state_dir:
        c.append(f"-v {args.state_dir}:/state")

    if args.local:
        c.append('localhost/btrfs-fuzz')
    else:
        c.append('dxuu/btrfs-fuzz')

    sh(' '.join(c))


def main():
    parser = argparse.ArgumentParser(
            prog='x',
            formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument(
            '--local',
            action='store_true',
            help='Use local docker image')
    parser.set_defaults(func=lambda _: parser.print_help())

    subparsers = parser.add_subparsers(help='subcommands')

    build = subparsers.add_parser('build', help='build btrfs-fuzz components')
    build.set_defaults(func=cmd_build)

    run = subparsers.add_parser('run', help='run fuzzer')
    run.set_defaults(func=cmd_run)

    shell = subparsers.add_parser('shell', help='start shell in VM')
    shell.add_argument(
            '-s',
            '--state-dir',
            type=str,
            help='Shared state directory between host and VM, mounted in VM at /state')
    shell.set_defaults(func=cmd_shell)

    help = subparsers.add_parser(
            'help',
            help='print help')
    help.set_defaults(func=lambda _: parser.print_help())

    args = parser.parse_args()
    args.func(args)

if __name__ == '__main__':
    proj_dir = pathlib.Path(__file__).parent
    os.chdir(proj_dir)

    main()
