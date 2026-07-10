#!/usr/bin/env bash
# Grep gate: scan the codebase for forbidden tokens and report
# them. By default it runs in **advisory** mode: it prints all
# findings and exits 0 so the CI gate stays green while we work
# through the backlog. Set `GREP_GATE_STRICT=1` to enable
# strict mode (fails CI on any violation).
#
# Categories:
#   - `static mut`           (forbidden everywhere)
#   - `.unwrap()`            (forbidden in production code)
#   - `unsafe {`             (only allowed in kernel + HAL impls)
#
# Allowlisted violations are loaded from
# `tools/grep_gate.allowlist`.

set -euo pipefail

cd "$(dirname "$0")/.."

allowlist="tools/grep_gate.allowlist"
strict="${GREP_GATE_STRICT:-0}"

violations=0
allowlisted=0
report() {
    echo "$1"
    violations=$((violations + 1))
}

filter_allowlist() {
    while IFS= read -r line; do
        if [[ -f "$allowlist" ]] && grep -Fqx "$line" "$allowlist"; then
            allowlisted=$((allowlisted + 1))
            continue
        fi
        echo "$line"
        violations=$((violations + 1))
    done
}

echo "=== grep_gate: static mut ==="
filter_allowlist < <(grep -RIn --include='*.rs' --exclude-dir=target -E 'static\s+mut' . || true)

echo "=== grep_gate: .unwrap() in production src ==="
for crate in caplet-kernel caplet-cap caplet-fs caplet-net caplet-mqttsn caplet-task caplet-sync caplet-async; do
    filter_allowlist < <(grep -RIn --include='*.rs' --exclude-dir=target \
                                  --exclude-dir=tests --exclude-dir=examples \
                                  -F ".unwrap()" "crates/$crate/src" || true)
done

echo "=== grep_gate: unsafe { outside kernel/HAL ==="
unsafe_dirs="crates/caplet-kernel/src|crates/caplet-hal-stm32f4/src|crates/caplet-hal-nrf52/src|ports/cortex-m/src"
filter_allowlist < <(grep -RIn --include='*.rs' --exclude-dir=target 'unsafe {' . \
                     | grep -Ev "^./($unsafe_dirs)/" || true)

echo "=== grep_gate summary ==="
echo "  violations : $violations"
echo "  allowlisted: $allowlisted"

if [[ "$strict" == "1" && "$violations" -gt 0 ]]; then
    echo "grep_gate STRICT: failing" >&2
    exit 1
fi
echo "grep_gate OK (advisory)"