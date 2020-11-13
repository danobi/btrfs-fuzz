# btrfs-fuzz

[![][0]][1]
[![][3]][4]

`btrfs-fuzz` is an unsupervised coverage guided-fuzzer tailored for [btrfs][2].

## Dependencies

`btrfs-fuzz` is mostly self-contained inside a docker image. The only things you
need on your host are:

* `btrfs-progs`
* [`podman`][5]
* python3
* QEMU
* Rust toolchain

## Quickstart

```shell
$ git clone https://github.com/danobi/btrfs-fuzz.git
$ cd btrfs-fuzz
$ ./x.py build
$ ./x.py seed
$ ./x.py run
```

## x.py

`x.py` is the "Makefile" for this project. See `x.py --help` for full options.

## Trophies

* [Kernel divide-by-zero][6]
* [Kernel stack scribbling][7]


[0]: https://img.shields.io/docker/cloud/build/dxuu/btrfs-fuzz
[1]: https://hub.docker.com/r/dxuu/btrfs-fuzz
[2]: https://en.wikipedia.org/wiki/Btrfs
[3]: https://github.com/danobi/btrfs-fuzz/workflows/Rust/badge.svg
[4]: https://github.com/danobi/btrfs-fuzz/actions?query=workflow%3ARust
[5]: https://podman.io/
[6]: https://lore.kernel.org/linux-btrfs/20201020173745.227665-1-dxu@dxuuu.xyz/
[7]: https://lore.kernel.org/linux-btrfs/0e869ff2f4ace0acb4bcfcd9a6fcf95d95b1d85a.1605232441.git.dxu@dxuuu.xyz/
