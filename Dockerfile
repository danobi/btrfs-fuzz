FROM alpine:edge as kernel

RUN apk update
RUN apk add \
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

RUN git clone --depth 1 https://github.com/torvalds/linux.git
WORKDIR linux

COPY scripts/config_kernel.sh config_kernel.sh
COPY configs/archlinux.config .config
RUN chmod +x config_kernel.sh
RUN ./config_kernel.sh

RUN make bzImage -j$(nproc)

# Second build stage builds statically linked btrfs-fuzz software components
FROM rust:alpine as btrfsfuzz

RUN apk update
RUN apk add musl-dev

WORKDIR /
RUN mkdir btrfs-fuzz
WORKDIR btrfs-fuzz
COPY Cargo.toml Cargo.lock .
RUN mkdir src
COPY src src
RUN cargo update
RUN cargo build --release

# Final stage build copies over binaries from build stages and only installs
# runtime components.
FROM aflplusplus/aflplusplus:latest

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update
RUN apt-get install -y \
  btrfs-progs \
  kmod \
  strace \
  qemu-system-x86

WORKDIR /
RUN mkdir btrfs-fuzz
WORKDIR btrfs-fuzz

RUN git clone https://github.com/amluto/virtme.git

COPY --from=kernel /linux/arch/x86/boot/bzImage .
COPY --from=btrfsfuzz /btrfs-fuzz/target/release/runner .

ENTRYPOINT ["virtme/virtme-run", "--kimg", "bzImage", "--rw", "--pwd", "--memory", "512M"]
