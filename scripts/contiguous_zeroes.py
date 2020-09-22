#!/bin/python3
#
# Calculates number of contiguous zeroes (>2 implies contiguous) in a binary
# file. This script is useful to determine if 0-encoding a filesystem image
# is useful.

import sys
import enum


class State(enum.Enum):
    ZERO = 0
    ONE = 1
    TWO = 2
    TWO_PLUS = 3


def bytes_from_file(filename, chunksize=8192):
    with open(filename, "rb") as f:
        while True:
            chunk = f.read(chunksize)
            if chunk:
                for b in chunk:
                    yield b
            else:
                break


def main():
    if len(sys.argv) != 2:
        print("Usage: ./contiguous_zeroes.py FILE")
        sys.exit(1)

    count = 0
    state = State.ZERO

    for b in bytes_from_file(sys.argv[1]):
        z = b == 0

        if state == State.ZERO:
            if z:
                state = State.ONE
        elif state == State.ONE:
            if z:
                state = State.TWO
            else:
                state = State.ZERO
        elif state == State.TWO:
            if z:
                # Add the previous two + current zero
                count += 3
                state = State.TWO_PLUS
            else:
                state = State.ZERO
        else:
            if z:
                count += 1
            else:
                state = State.ZERO

    print(count)


if __name__ == "__main__":
    main()
