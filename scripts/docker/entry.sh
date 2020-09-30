#!/bin/bash
#
# Entry point for container

set -eu

cd /btrfs-fuzz
./virtme/virtme-run --kimg bzImage --rw --pwd --memory 1024M "$@"
