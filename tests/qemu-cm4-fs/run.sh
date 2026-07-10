#!/usr/bin/env bash
# Run the hello-qemu-cm4-fs board in QEMU.
#
# Boots QEMU with the mps2-an386 machine (Cortex-M4 with FPU)
# and pipes semihosting output back through QEMU's chardev.
#
# Expected output on stdout:
#   boot: hello-qemu-cm4-fs booting
#   fs:   VFS demo task started
#   fs:   open /tmp/sentinel.txt -> h=N
#   fs:   parsed mount='tmp' sub='/sentinel.txt'
#   fs:   VFS demo task done
#
# Exit codes:
#   0   — demo ran cleanly (QEMU exited via semihosting HALT)
#   124 — timeout (50 s)
#   *   — QEMU crashed

set -euo pipefail

cd "$(dirname "$0")/../.."

target="thumbv7em-none-eabihf"

echo "[qemu-cm4-fs] building..."
cargo build --release --target "$target" -p hello-qemu-cm4-fs

binary="target/$target/release/hello-qemu-cm4-fs"
if [[ ! -f "$binary" ]]; then
    echo "binary not found at $binary" >&2
    exit 1
fi

echo "[qemu-cm4-fs] running qemu..."
timeout 50s qemu-system-arm \
    -machine mps2-an386 \
    -cpu cortex-m4 \
    -kernel "$binary" \
    -nographic \
    -semihosting-config enable=on,target=native \
    -serial mon:stdio 2>&1 | tee /tmp/qemu-cm4-fs.log

# Inspect the log for the sentinel line. The demo prints
# "VFS demo task done" only if the FS mount + open + close
# sequence all succeeded.
if grep -q "VFS demo task done" /tmp/qemu-cm4-fs.log; then
    echo "[qemu-cm4-fs] PASS"
    exit 0
else
    echo "[qemu-cm4-fs] FAIL: sentinel line not found" >&2
    exit 1
fi