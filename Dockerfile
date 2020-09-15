FROM alpine:edge

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
