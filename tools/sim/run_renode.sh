#!/usr/bin/env bash
# Run one CapletRT board under Renode, assert its boot marker, and keep
# the transcript plus UART capture under target/sim/<board>/.
#
# The .resc files use @REPL@, @ELF@ and @OUT@ placeholders so the same
# checked-in script works on every host and in CI.
#
# Usage: tools/sim/run_renode.sh <board>
# Environment: RENODE (path to renode), SIM_TIMEOUT (seconds, default 20).

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

[ $# -eq 1 ] || die "usage: $0 <board>"
load_board "$1"

build_board release
mkdir -p "$BOARD_OUT"
RENODE_LOG="$BOARD_OUT/renode.log"
UART_OUT="$BOARD_OUT/uart.out"
RENDERED="$BOARD_OUT/run.resc"
: > "$RENODE_LOG"
: > "$UART_OUT"

RENODE_BIN="$(find_program RENODE renode)" || die "Renode not found (set RENODE)"
REPL_PATH="$PROJECT_ROOT/$REPL"
RESC_PATH="$PROJECT_ROOT/$RESC"
[ -f "$REPL_PATH" ] || die "missing Renode platform description: $REPL_PATH"
[ -f "$RESC_PATH" ] || die "missing Renode script: $RESC_PATH"

sed -e "s|@REPL@|@$REPL_PATH|g" \
    -e "s|@ELF@|@$BIN|g" \
    -e "s|@OUT@|@$UART_OUT|g" \
    "$RESC_PATH" > "$RENDERED"

info "Renode: $RENODE_BIN --console --disable-gui -P -1"
info "  repl: $REPL_PATH"
info "  elf:  $BIN"
renode_cmd=("$RENODE_BIN" --console --disable-gui -P -1 --plain \
    -e "include @$RENDERED")
# Renode 1.17 initialises its console through System.Console and throws
# `Success` when stdin is a pipe without a controlling terminal.  A
# pseudo-terminal is enough and keeps the monitor transcript intact.
# Developers running the script interactively keep their real TTY.
if [ ! -t 0 ] && command -v script >/dev/null 2>&1; then
    printf -v quoted '%q ' "${renode_cmd[@]}"
    timeout "${SIM_TIMEOUT:-20}" script -qec "$quoted" /dev/null \
        >"$RENODE_LOG" 2>&1
else
    timeout "${SIM_TIMEOUT:-20}" "${renode_cmd[@]}" >"$RENODE_LOG" 2>&1
fi
rc=$?
if [ "$rc" -ne 0 ]; then
    printf 'Renode exited with rc=%s\n' "$rc" >&2
fi

print_log_excerpt "$RENODE_LOG" 40
print_log_excerpt "$UART_OUT" 40

# Renode returns 0 even for monitor command errors, so inspect the log.
if grep -Eq "There was an error executing command|Could not tokenize|Error E[0-9]+:|Unhandled exception" "$RENODE_LOG"; then
    die "Renode reported a platform/command error"
fi
if grep -Eqi "unhandled (peripheral|memory) access|unhandled access to (peripheral|memory)|\b[1-9][0-9]* unhandled (peripheral|memory) access" "$RENODE_LOG"; then
    die "Renode reported unhandled peripheral accesses"
fi

assert_board_log "$UART_OUT" || exit 1
info "PASS [$BOARD_KEY] Renode: $EXPECT"
