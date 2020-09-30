#!/bin/bash
#
# Entry point for container

set -eu

./virtme/virtme-run --kimg bzImage --rw --pwd --memory 1024M "$@"
