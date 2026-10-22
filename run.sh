#!/usr/bin/env bash
set -euo pipefail

# Change to the workspace root
cd /I/CapletRT

echo "Running QEMU hello-allwinner-a20-qemu test..."

# Build first the specific package with target-none feature
cargo build --release -p hello-allwinner-a20-qemu --features target-none

# Run QEMU with absolute path, redirecting all output to a file
"/c/Program Files/qemu/qemu-system-arm.exe" \
    -machine vexpress-a9 \
    -nographic \
    -semihosting-config enable=on,target=native \
    -kernel "$(pwd)/target/armv7a-none-eabihf/release/hello-allwinner-a20-qemu" \
    -serial stdio \
    1> out.txt 2>&1 &

QEMU_PID=$!
sleep 3
kill $QEMU_PID 2>/dev/null || true

# Check output
echo "=== QEMU Output ==="
cat out.txt || echo "No output captured"

if grep -q "Hello from Allwinner A20" out.txt 2>/dev/null; then
    echo "PASS: Expected output found"
    exit 0
else
    echo "FAIL: Expected output not found"
    exit 1
fi