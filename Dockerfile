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

# Second build stage only keeps bzImage and drops everything else

FROM aflplusplus/aflplusplus:latest

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update
RUN apt-get install -y qemu-system-x86

WORKDIR /

COPY --from=kernel /linux/arch/x86/boot/bzImage /bzImage
RUN git clone https://github.com/amluto/virtme.git
