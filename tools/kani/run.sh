#!/usr/bin/env bash
# Run the complete CapletRT Kani catalogue.
#
# Kani selects harnesses by Rust function name, not by source-file path.
# A missing verifier is a hard, explicit "UNVERIFIED" result: it must never
# look like a successful CI run.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"

if ! command -v cargo-kani >/dev/null 2>&1; then
    echo "UNVERIFIED: cargo-kani is not on PATH" >&2
    echo "            install with: cargo install --locked kani-verifier && cargo-kani setup" >&2
    exit 127
fi

cd "$ROOT"
echo "=== Kani: caplet-kani-harnesses (entire catalogue) ==="
if ! cargo kani -p caplet-kani-harnesses "$@"; then
    echo "FAIL: Kani catalogue did not verify" >&2
    exit 1
fi

echo "PASS: Kani catalogue verified"
