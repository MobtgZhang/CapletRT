#!/usr/bin/env bash
# Tickless-idle verification script.
#
# Builds the deterministic `tickless-verify` image and asserts the
# counters emitted over semihosting by QEMU. This is a real runtime
# assertion: a missing QEMU, a build failure, a bad image, or a
# `tickless: FAILED` line is a hard failure.
#
# Usage:
#   tools/tickless_verify.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="thumbv7em-none-eabihf"
TIMEOUT_SEC="${TIMEOUT_SEC:-5}"
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

if ! command -v qemu-system-arm >/dev/null 2>&1; then
    echo "[tickless] ERROR: qemu-system-arm is required" >&2
    exit 1
fi

echo "[tickless] building"
cargo build -p hello-qemu-cm4 --target "$TARGET" --release \
    --features target-none,tickless-verify

ELF="$ROOT/target/$TARGET/release/hello-qemu-cm4"

if [[ ! -f "$ELF" ]]; then
    echo "[tickless] ERROR: expected ELF not found: $ELF" >&2
    exit 1
fi

echo "[tickless] running on QEMU (mps2-an386) for ${TIMEOUT_SEC}s"
set +e
timeout "$TIMEOUT_SEC" qemu-system-arm \
    -machine mps2-an386 \
    -cpu cortex-m4 \
    -nographic \
    -semihosting-config enable=on,target=native \
    -kernel "$ELF" \
    >"$LOG" 2>&1
status=$?
set -e

cat "$LOG"

if [[ "$status" -ne 0 && "$status" -ne 124 && "$status" -ne 137 ]]; then
    echo "[tickless] ERROR: QEMU failed with exit status $status" >&2
    exit "$status"
fi

if grep -qF "tickless: FAILED" "$LOG"; then
    echo "[tickless] ERROR: diagnostic reported FAILED" >&2
    exit 1
fi
if ! grep -qF "tickless: wakeups=5 idle_ticks=100 irqs=5" "$LOG"; then
    echo "[tickless] ERROR: expected deterministic counters were not observed" >&2
    exit 1
fi
if ! grep -qF "task_table: linked=4 explicit=2 launched=2 skipped=0" "$LOG"; then
    echo "[tickless] ERROR: static/explicit task table registration failed" >&2
    exit 1
fi
if ! grep -qF "tickless: OK" "$LOG"; then
    echo "[tickless] ERROR: missing 'tickless: OK' marker" >&2
    exit 1
fi

echo "[tickless] OK"
