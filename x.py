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
    sh('podman pull dxuu/btrfs-fuzz')
    sh('cargo build')


def cmd_run(args):
    print("Starting btrfs-fuzz")


def main():
    parser = argparse.ArgumentParser(
            prog='x',
            formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.set_defaults(func=lambda _: parser.print_help())

    subparsers = parser.add_subparsers(help='subcommands')

    build = subparsers.add_parser('build', help='build btrfs-fuzz components')
    build.set_defaults(func=cmd_build)

    run = subparsers.add_parser('run', help='run fuzzer')
    run.set_defaults(func=cmd_run)

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
