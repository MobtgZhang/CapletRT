#!/usr/bin/env bash
# Run one CapletRT board under QEMU, assert its boot marker, and keep the
# full serial/semihosting transcript under target/sim/<board>/qemu.log.
#
# Usage: tools/sim/run_qemu.sh <board>
# Environment: QEMU_SYSTEM_ARM / QEMU_SYSTEM_AARCH64 /
#              QEMU_SYSTEM_RISCV32 / QEMU_SYSTEM_RISCV64,
#              SIM_TIMEOUT (seconds, default 8), QEMU_SMP (A-profile only).

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

[ $# -eq 1 ] || die "usage: $0 <board>"
load_board "$1"

build_board release
mkdir -p "$BOARD_OUT"
LOG="$BOARD_OUT/qemu.log"
: > "$LOG"

QEMU="$(find_program "$QEMU_VAR" $QEMU_NAMES)" || \
    die "QEMU not found (set $QEMU_VAR)"

qemu_board_args

info "QEMU: $QEMU ${QEMU_ARGS[*]}"
# `-nographic` multiplexes the guest serial port onto stdio.  Detach
# stdin so an interactive developer shell cannot make QEMU select the
# controlling terminal for its character backend; otherwise the
# transcript is empty even though the guest booted successfully.
timeout "${SIM_TIMEOUT:-8}" "$QEMU" "${QEMU_ARGS[@]}" </dev/null >"$LOG" 2>&1
rc=$?
# 124/137 mean timeout killed a guest that idled after printing, which is
# normal for these bare-metal images.  The output assertion is authoritative.
if [ "$rc" -ne 0 ] && [ "$rc" -ne 124 ] && [ "$rc" -ne 137 ]; then
    printf 'QEMU exited with rc=%s\n' "$rc" >&2
fi

print_log_excerpt "$LOG" 80
assert_board_log "$LOG" || exit 1
info "PASS [$BOARD_KEY] QEMU: $EXPECT"
