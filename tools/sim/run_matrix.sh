#!/usr/bin/env bash
# Run every supported board through QEMU.  A failure in one board does
# not prevent the remaining boards from being tested; the final exit
# status is non-zero if any board failed.

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

boards=("$@")
if [ ${#boards[@]} -eq 0 ]; then
    mapfile -t boards < <(all_boards)
fi

failed=()
for board in "${boards[@]}"; do
    info "=================================================================="
    info "QEMU matrix: $board"
    info "=================================================================="
    if ! "$SIM_DIR/run_qemu.sh" "$board"; then
        failed+=("$board")
    fi
done

if [ ${#failed[@]} -ne 0 ]; then
    printf 'FAIL: QEMU matrix failures: %s\n' "${failed[*]}" >&2
    exit 1
fi
info "PASS: QEMU matrix (${#boards[@]} boards)"
