#!/usr/bin/env bash
# Kani harness runner for the CapletRT kernel.
#
# Walks tools/kani/harness/ and runs `cargo kani` on every .rs file.
# Stops on the first failure so CI can gate the build.
#
# Kani is not yet installed in the development environment. When it
# is, install with:
#
#     cargo install --locked kani-verifier
#     cargo-kani setup
#
# Then run this script from the workspace root.

set -euo pipefail

# Resolve the workspace root from this script's path.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"

HARNESS_DIR="$HERE/harness"

if ! command -v cargo-kani >/dev/null 2>&1; then
    echo "error: cargo-kani is not on PATH." >&2
    echo "       install with: cargo install --locked kani-verifier && cargo-kani setup" >&2
    exit 127
fi

if [[ ! -d "$HARNESS_DIR" ]]; then
    echo "no harness directory at $HARNESS_DIR; nothing to do." >&2
    exit 0
fi

cd "$ROOT"

pass=0
fail=0
shopt -s nullglob
for harness in "$HARNESS_DIR"/*.rs; do
    name="$(basename "$harness" .rs)"
    echo
    echo "=== $name ==="
    if cargo kani --harness "$harness"; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1))
        echo "FAILED: $name" >&2
    fi
done

echo
echo "Kani summary: $pass passed, $fail failed."

if [[ $fail -gt 0 ]]; then
    exit 1
fi