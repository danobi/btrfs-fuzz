# btrfs-fuzz

[![][0]][1]
[![][3]][4]

`btrfs-fuzz` is an unsupervised coverage guided-fuzzer tailored for [btrfs][2].

## Dependencies

`btrfs-fuzz` is mostly self-contained inside a docker image. The only things you
need on your host is:

* [`podman`][5]
* QEMU
* python3

## Quickstart

```shell
$ git clone https://github.com/danobi/btrfs-fuzz.git
$ cd btrfs-fuzz
$ ./x.py build
$ ./x.py seed
$ ./x.py run ./_state
```

## x.py

`x.py` is the "Makefile" for this project. See `x.py --help` for full options.


[0]: https://img.shields.io/docker/cloud/build/dxuu/btrfs-fuzz
[1]: https://hub.docker.com/r/dxuu/btrfs-fuzz
[2]: https://en.wikipedia.org/wiki/Btrfs
[3]: https://github.com/danobi/btrfs-fuzz/workflows/Rust/badge.svg
[4]: https://github.com/danobi/btrfs-fuzz/actions?query=workflow%3ARust
[5]: https://podman.io/
