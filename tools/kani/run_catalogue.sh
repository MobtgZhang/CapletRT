#!/usr/bin/env bash
# Run every `#[kani::proof]` function individually so one failing proof
# cannot hide failures in the rest of the catalogue.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HARNESS_DIR="$ROOT/tools/kani/harness"

if ! command -v cargo-kani >/dev/null 2>&1; then
    echo "UNVERIFIED: cargo-kani is not on PATH" >&2
    exit 127
fi

# A single malformed harness must not take down the developer machine.  Kani
# runs one proof at a time in this script, so this address-space cap applies to
# the whole verifier subtree.  Override it explicitly for a known-heavy proof.
kani_memory_limit_kib="${KANI_MEMORY_LIMIT_KIB:-8388608}"
ulimit -v "$kani_memory_limit_kib" || {
    echo "FAIL: cannot apply Kani memory limit (${kani_memory_limit_kib} KiB)" >&2
    exit 1
}
echo "Kani memory limit: $((kani_memory_limit_kib / 1024)) MiB"

[ -d "$HARNESS_DIR" ] || {
    echo "FAIL: no harness directory at $HARNESS_DIR" >&2
    exit 1
}

mapfile -t harnesses < <(
    rg -U -o --no-filename --replace '$1' \
        '#\[kani::proof\](?:\s*#\[[^\n]+\])*\s*\n\s*fn\s+([A-Za-z_][A-Za-z0-9_]*)' \
        "$HARNESS_DIR"/*.rs
)

if [ "${#harnesses[@]}" -eq 0 ]; then
    echo "FAIL: no Kani proof functions found" >&2
    exit 1
fi

cd "$ROOT"
failed=()
for harness in "${harnesses[@]}"; do
    echo "=== Kani: $harness ==="
    if ! cargo kani -p caplet-kani-harnesses --harness "$harness"; then
        failed+=("$harness")
        echo "FAIL: $harness" >&2
    fi
done

if [ "${#failed[@]}" -ne 0 ]; then
    printf 'FAIL: %d Kani proof(s) failed: %s\n' \
        "${#failed[@]}" "${failed[*]}" >&2
    exit 1
fi

echo "PASS: Kani catalogue (${#harnesses[@]} proofs)"
