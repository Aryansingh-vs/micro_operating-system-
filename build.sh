#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
cargo build -p aryos
cargo run -p aryos -- image

printf "AryOS image size: "
stat -f%z dist/aryos-bios.img 2>/dev/null || stat -c%s dist/aryos-bios.img
printf " bytes (BIOS)\n"

printf "UEFI image size: "
stat -f%z dist/aryos-uefi.img 2>/dev/null || stat -c%s dist/aryos-uefi.img
printf " bytes\n"
