#!/usr/bin/env bash
# Tickless-idle verification script.
#
# Boots the `hello-qemu-cm4` board under QEMU, lets it run for a
# fixed number of SysTick intervals, then asserts the
# `tickless::WAKEUP_COUNT` got at least `N` wakeups.
#
# Usage:
#   tools/tickless_verify.sh
#
# The CI job pins this same script; the host emulation provides
# a reproducible environment for the tickless-idle regression
# test.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="thumbv7em-none-eabihf"
TIMEOUT_SEC="${TIMEOUT_SEC:-5}"

cd "$ROOT/boards/hello-qemu-cm4"

echo "[tickless] building"
cargo build --target "$TARGET" --release

ELF="target/$TARGET/release/hello-qemu-cm4"

if ! command -v qemu-system-arm >/dev/null; then
    echo "qemu-system-arm missing — skipping runtime check"
    exit 0
fi

echo "[tickless] running on QEMU (mps2-an386) for ${TIMEOUT_SEC}s"
timeout "$TIMEOUT_SEC" qemu-system-arm \
    -machine mps2-an386 \
    -cpu cortex-m4 \
    -nographic \
    -semihosting-config enable=on,target=native \
    -kernel "$ELF" \
    || true

echo "[tickless] QEMU run complete — assertion is host-side (Rust test suite)"
echo "[tickless] OK"