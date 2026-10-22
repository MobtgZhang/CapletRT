#!/usr/bin/env bash
# Run every supported board through Renode.  See run_matrix.sh for the
# failure-aggregation policy.

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

boards=("$@")
if [ ${#boards[@]} -eq 0 ]; then
    mapfile -t boards < <(all_boards)
fi

failed=()
for board in "${boards[@]}"; do
    info "=================================================================="
    info "Renode matrix: $board"
    info "=================================================================="
    if ! "$SIM_DIR/run_renode.sh" "$board"; then
        failed+=("$board")
    fi
done

if [ ${#failed[@]} -ne 0 ]; then
    printf 'FAIL: Renode matrix failures: %s\n' "${failed[*]}" >&2
    exit 1
fi
info "PASS: Renode matrix (${#boards[@]} boards)"
