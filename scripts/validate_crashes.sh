#!/bin/bash
#
# Script to see if discovered crashes can reproduce.
#
# Env vars:
#     REMOTE: use `x.py --remote`
#
# Example:
#     REMOTE=1 ./validate_crashes.sh ./_state/output/crashes
#

set -eu

if [[ "$#" != 1 ]]; then
  echo 'Usage: validate_crashes.sh DIR' >> /dev/stderr
  exit 1
fi

if [[ -v REMOTE ]]; then
  cmd="./x.py --remote"
else
  cmd="./x.py"
fi

for f in "$1"/*; do
  echo "Testing ${f}"
  output=$($cmd repro --exit "$f")
  if echo "$output" | grep -q -e FAILURE -e "RIP:" -e "Call Trace:"; then
    echo -e "\tReproduced failure for ${f}"
  fi
done
