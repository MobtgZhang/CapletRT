#!/usr/bin/env bash
# Kani harness runner. Walks every .rs file under
# tools/kani/harness and runs `cargo kani --harness` on each,
# gracefully degrading if cargo-kani is not installed.

set -euo pipefail

cd "$(dirname "$0")/../.."

if ! command -v cargo-kani >/dev/null 2>&1; then
    echo "cargo-kani not available — Kani gate SKIPPED."
    exit 0
fi

harness_dir="tools/kani/harness"
if [[ ! -d "$harness_dir" ]]; then
    echo "no harness directory at $harness_dir" >&2
    exit 1
fi

shopt -s nullglob
harnesses=( "$harness_dir"/*.rs )
shopt -u nullglob

if (( ${#harnesses[@]} == 0 )); then
    echo "no Kani harnesses found"
    exit 0
fi

fail=0
for h in "${harnesses[@]}"; do
    echo "=== Kani: $h ==="
    if ! cargo kani --harness "$h"; then
        echo "FAIL: $h" >&2
        fail=$((fail + 1))
    fi
done

if (( fail > 0 )); then
    echo "Kani gate: $fail harness(es) failed" >&2
    exit 1
fi
echo "Kani gate: OK (${#harnesses[@]} harnesses)"