#!/bin/bash
# QEMU Cortex-M3 integration test

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "Running QEMU Cortex-M3 test..."

# Build first
cd "$PROJECT_ROOT"
cargo build --release --bin hello-qemu-cm3 2>/dev/null || cargo build --release -p hello-qemu-cm3

# Run QEMU
qemu-system-arm \
    -machine mps2-an385 \
    -cpu cortex-m3 \
    -nographic \
    -semihosting-config enable=on,target=cli \
    -kernel "$PROJECT_ROOT/boards/hello-qemu-cm3/target/thumbv6m-none-eabi/release/hello-qemu-cm3.elf" \
    -serial file:"$SCRIPT_DIR/out.txt" &

QEMU_PID=$!
sleep 3
kill $QEMU_PID 2>/dev/null || true

# Check output
echo "=== QEMU Output ==="
cat "$SCRIPT_DIR/out.txt" || echo "No output captured"

if grep -q "hello world" "$SCRIPT_DIR/out.txt" 2>/dev/null; then
    echo "PASS: hello world found in output"
    exit 0
else
    echo "FAIL: hello world not found in output"
    exit 1
fi
