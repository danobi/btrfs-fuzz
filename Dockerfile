FROM alpine:edge as kernel

ARG KERNEL_REPO=https://github.com/torvalds/linux.git
ARG KERNEL_BRANCH=master

ENV KERNEL_REPO=${KERNEL_REPO}
ENV KERNEL_BRANCH=${KERNEL_BRANCH}

RUN apk update && apk add \
  bash \
  bison \
  build-base \
  diffutils \
  elfutils-dev \
  findutils \
  flex \
  git \
  gzip \
  linux-headers \
  perl \
  python3 \
  openssl \
  openssl-dev \
  xz

WORKDIR /

RUN git clone --depth 1 ${KERNEL_REPO} linux --branch ${KERNEL_BRANCH}
WORKDIR linux

COPY scripts/docker/config_kernel.sh config_kernel.sh
COPY configs/archlinux.config .config
RUN chmod +x config_kernel.sh
RUN ./config_kernel.sh

RUN make bzImage -j$(nproc)

# Second build stage builds statically linked btrfs-fuzz software components
FROM rust:alpine as btrfsfuzz

RUN apk update && apk add musl-dev

WORKDIR /
RUN mkdir btrfs-fuzz
WORKDIR btrfs-fuzz
COPY Cargo.toml Cargo.lock ./
RUN mkdir src
COPY src src
RUN cargo update
RUN cargo build --release -p runner

# Third stage builds dynamically linked btrfs-fuzz components
FROM rust:latest as btrfsfuzz-dy

WORKDIR /
RUN mkdir btrfs-fuzz
WORKDIR btrfs-fuzz
COPY Cargo.toml Cargo.lock ./
RUN mkdir src
COPY src src
RUN cargo update
RUN cargo build --release -p mutator

# Final stage build copies over binaries from build stages and only installs
# runtime components.
FROM aflplusplus/aflplusplus:latest

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y \
  btrfs-progs \
  busybox \
  kmod \
  linux-tools-generic \
  less \
  strace \
  qemu-system-x86

WORKDIR /
RUN mkdir btrfs-fuzz
WORKDIR btrfs-fuzz

COPY scripts/docker/entry.sh entry.sh
RUN chmod +x entry.sh

RUN git clone https://github.com/amluto/virtme.git

COPY --from=kernel /linux/arch/x86/boot/bzImage .
COPY --from=kernel /linux/vmlinux .
COPY --from=btrfsfuzz /btrfs-fuzz/target/release/runner .
COPY --from=btrfsfuzz-dy /btrfs-fuzz/target/release/libmutator.so .

ENTRYPOINT ["./entry.sh"]
