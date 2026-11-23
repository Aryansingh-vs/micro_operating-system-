#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
./build.sh
qemu-system-x86_64 \
  -m 128M \
  -no-reboot \
  -drive format=raw,file=dist/aryos-bios.img
