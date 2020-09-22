#!/bin/python3
#
# Calculates number of contiguous zeroes (controlled by minimum WIDTH zeroes)
# in a binary file. This script is useful to determine if 0-encoding a
# filesystem image is useful.

import sys
import enum


def bytes_from_file(filename, chunksize=(16 << 10)):
    with open(filename, "rb") as f:
        while True:
            chunk = f.read(chunksize)
            if chunk:
                for b in chunk:
                    yield b
            else:
                break


def main():
    if len(sys.argv) < 2:
        print("Usage: ./contiguous_zeroes.py FILE [WIDTH]=16")
        sys.exit(1)

    width = 16
    if len(sys.argv) == 3:
        width = int(sys.argv[2])

    count = 0
    chunks = 0
    total_bytes = 0
    state = 0

    for b in bytes_from_file(sys.argv[1]):
        total_bytes += 1

        if b == 0:
            state += 1
            if state < width:
                pass
            elif state == width:
                count += width
                chunks += 1
            else:  # state > width
                count += 1
        else:
            state = 0

    print(f"{count:<12}eligible zeros")
    print(f"{chunks:<12}chunks of zeroes")
    print(f"{total_bytes:<12}total bytes")
    print("------------------------------")

    metadata_bytes = 4 * chunks
    compressed_size = total_bytes - count + metadata_bytes
    compression_ratio = total_bytes / compressed_size

    print(f"{compressed_size >> 10:.3f}KB compressed size")
    print(f"{compression_ratio:.3f} compression ratio")


if __name__ == "__main__":
    main()
