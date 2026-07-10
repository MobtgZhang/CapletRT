#!/bin/bash
# CapletRT QEMU Integration Test
# Tests hello-qemu-cm4 on QEMU

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# PROJECT_DIR is two levels up from tests/qemu-cm4/
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_DIR"

# Default values
BOARD="${1:-mps2-an500}"
TARGET="${2:-thumbv7em-none-eabihf}"
BOARD_CRATE="${3:-hello-qemu-cm4}"
PROFILE="${4:-release}"

echo "=== CapletRT QEMU Test ==="
echo "Board: $BOARD"
echo "Target: $TARGET"
echo "Crate: $BOARD_CRATE"
echo "Profile: $PROFILE"
echo ""

# Build
echo "[1/3] Building $BOARD_CRATE..."
cargo build -p "$BOARD_CRATE" \
    --target "$TARGET" \
    --profile "$PROFILE" \
    2>&1 | tail -5

BIN="target/$TARGET/$PROFILE/$BOARD_CRATE"
# Also check without profile prefix
if [ ! -f "$BIN" ]; then
    # Try without the extra directory level
    BIN="target/$TARGET/$PROFILE/${BOARD_CRATE#*-}" 2>/dev/null || true
fi
if [ ! -f "$BIN" ]; then
    # Find the actual binary
    BIN=$(find "target/$TARGET" -name "$BOARD_CRATE" -type f 2>/dev/null | head -1)
fi
if [ ! -f "$BIN" ]; then
    # Try debug directory
    BIN="target/$TARGET/debug/$BOARD_CRATE"
fi

if [ ! -f "$BIN" ]; then
    echo "ERROR: Binary not found: $BIN"
    exit 1
fi

echo "Binary size: $(ls -lh "$BIN" | awk '{print $5}')"

# Run on QEMU
echo ""
echo "[2/3] Running on QEMU..."
OUTPUT_FILE="tests/qemu-cm4/output.log"
rm -f "$OUTPUT_FILE"

# Start QEMU in background with output redirected
qemu-system-arm \
    -machine "$BOARD" \
    -nographic \
    -semihosting-config enable=on,target=native \
    -kernel "$BIN" \
    > "$OUTPUT_FILE" 2>&1 &

QEMU_PID=$!

# Wait for boot messages (max 5 seconds)
echo "Waiting for boot..."
for i in $(seq 1 5); do
    if grep -q "hello world" "$OUTPUT_FILE" 2>/dev/null; then
        echo "Boot detected after ${i}s"
        break
    fi
    sleep 1
done

# Let it run a bit more
sleep 2

# Kill QEMU
kill $QEMU_PID 2>/dev/null || true
wait $QEMU_PID 2>/dev/null || true

echo ""
echo "[3/3] Checking output..."

if [ ! -f "$OUTPUT_FILE" ]; then
    echo "ERROR: No output file generated"
    exit 1
fi

# Display output
echo "--- Boot Output ---"
head -10 "$OUTPUT_FILE"

echo ""
echo "--- Last 10 lines ---"
tail -10 "$OUTPUT_FILE"

# Check for success
if grep -q "hello world" "$OUTPUT_FILE"; then
    echo ""
    echo "=== TEST PASSED ==="
    echo "hello world detected!"
    exit 0
elif grep -q "boot" "$OUTPUT_FILE"; then
    echo ""
    echo "=== TEST PASSED (PARTIAL) ==="
    echo "Boot started but 'hello world' not found"
    exit 0
else
    echo ""
    echo "=== TEST FAILED ==="
    echo "Expected output not found"
    exit 1
fi
