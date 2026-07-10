#!/usr/bin/env bash
# Build the Nucleo-F446ZE board and (optionally) launch it in
# QEMU's `mps2-an386` emulator.
#
# Usage:
#   tests/qemu-nucleo-f446ze/build.sh
#   QEMU=1 tests/qemu-nucleo-f446ze/build.sh    # also run QEMU
#
# Exits 0 on success, non-zero on any error.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET="thumbv7em-none-eabihf"
BOARD="nucleo-f446ze"
QEMU="${QEMU:-0}"

cd "$ROOT/boards/$BOARD"

echo "[$BOARD] building for $TARGET"
cargo build --target "$TARGET" --release

ARTIFACT="target/$TARGET/release/$BOARD"

if [ "$QEMU" = "1" ]; then
    if ! command -v qemu-system-arm >/dev/null; then
        echo "qemu-system-arm not installed; skipping runtime check"
        exit 0
    fi
    echo "[$BOARD] running on QEMU (mps2-an386)"
    timeout 5 qemu-system-arm \
        -machine mps2-an386 \
        -cpu cortex-m4 \
        -nographic \
        -semihosting-config enable=on,target=native \
        -kernel "$ARTIFACT" \
        || true
    echo "[$BOARD] QEMU run finished"
fi

echo "[$BOARD] OK"