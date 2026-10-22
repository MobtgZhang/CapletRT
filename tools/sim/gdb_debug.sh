#!/usr/bin/env bash
# Launch a target-architecture GDB against QEMU or Renode.
#
# This script never pretends debugging works: it only runs when a GDB
# that understands the selected architecture is actually installed.
#
# Usage:
#   tools/sim/gdb_debug.sh <board> [--renode] [extra GDB arguments...]
#
# Environment: GDB (explicit override), QEMU_* variables, GDB_PORT.

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

[ $# -ge 1 ] || die "usage: $0 <board> [--renode] [gdb args...]"
load_board "$1"
shift

backend=qemu
if [ "${1:-}" = "--renode" ]; then
    backend=renode
    shift
fi

case "$ARCH" in
    riscv32|riscv64)
        arch_name=$([ "$ARCH" = riscv32 ] && echo riscv:rv32 || echo riscv:rv64)
        gdb_names=(gdb-multiarch riscv64-unknown-elf-gdb riscv64-linux-gnu-gdb gdb)
        ;;
    aarch64)
        arch_name=aarch64
        gdb_names=(gdb-multiarch aarch64-none-elf-gdb aarch64-linux-gnu-gdb gdb)
        ;;
    armv7a-a9|armv7a-a20|armv7m|armv7em|stm32f4)
        arch_name=arm
        gdb_names=(gdb-multiarch arm-none-eabi-gdb gdb) ;;
    *) die "no GDB mapping for architecture $ARCH" ;;
esac

# A binary being installed is not enough: the host `gdb` in many
# distributions only understands the native architecture.  Probe the
# actual target, and keep looking when the answer is "Undefined item".
gdb_supports_arch() {
    "$1" -q -batch -ex "set architecture $arch_name" >/dev/null 2>&1
}

resolve_gdb() {
    local candidate="$1"
    if [ -n "${GDB:-}" ]; then
        printf '%s\n' "$candidate"
    elif [ "${candidate#*/}" != "$candidate" ]; then
        [ -x "$candidate" ] && printf '%s\n' "$candidate"
    else
        command -v "$candidate" 2>/dev/null || true
    fi
}

GDB_BIN=""
checked=()
if [ -n "${GDB:-}" ]; then
    candidates=("$GDB")
else
    candidates=("${gdb_names[@]}")
fi
for candidate in "${candidates[@]}"; do
    path="$(resolve_gdb "$candidate")"
    [ -n "$path" ] || continue
    if gdb_supports_arch "$path"; then
        GDB_BIN="$path"
        break
    fi
    checked+=("$path")
done

if [ -z "$GDB_BIN" ]; then
    printf 'SKIP: no architecture-capable GDB found for %s (%s).\n' "$ARCH" "$arch_name" >&2
    printf '      looked for: %s\n' "${candidates[*]}" >&2
    if [ ${#checked[@]} -ne 0 ]; then
        printf '      rejected (target architecture unsupported): %s\n' "${checked[*]}" >&2
    fi
    printf '      install gdb-multiarch (Debian/Ubuntu) or set GDB=/path/to/gdb-multiarch.\n' >&2
    exit 2
fi

build_board dev
PORT="${GDB_PORT:-1234}"
mkdir -p "$BOARD_OUT"

if [ "$backend" = qemu ]; then
    QEMU="$(find_program "$QEMU_VAR" $QEMU_NAMES)" || die "QEMU not found (set $QEMU_VAR)"
    qemu_board_args
    info "Starting QEMU gdbstub on :$PORT ..."
    if command -v setsid >/dev/null 2>&1; then
        setsid "$QEMU" "${QEMU_ARGS[@]}" -gdb "tcp::$PORT" -S \
            >"$BOARD_OUT/gdb-qemu.log" 2>&1 &
    else
        "$QEMU" "${QEMU_ARGS[@]}" -gdb "tcp::$PORT" -S \
            >"$BOARD_OUT/gdb-qemu.log" 2>&1 &
    fi
    emulator_pid=$!
else
    RENODE_BIN="$(find_program RENODE renode)" || die "Renode not found (set RENODE)"
    REPL_PATH="$PROJECT_ROOT/$REPL"
    GDB_RESC="$BOARD_OUT/gdb.resc"
    # Pause the machine at reset so GDB can install breakpoints on early
    # symbols such as _start/rust_main before the boot path executes.
    cat > "$GDB_RESC" <<EOF
mach create "$BOARD_KEY-gdb"
machine LoadPlatformDescription @$REPL_PATH
sysbus LoadELF @$BIN
machine StartGdbServer $PORT true
# Keep the machine halted at reset until GDB has installed its
# breakpoints.  GDB's continue command resumes execution.
pause
EOF
    info "Starting Renode gdbstub on :$PORT ..."
    renode_cmd=("$RENODE_BIN" --console --disable-gui -P -1 --plain \
        -e "include @$GDB_RESC")
    # Renode's monitor exits when stdin reaches EOF.  Feed it from an
    # endless, non-blocking source so the emulation main loop keeps running
    # after the .resc finishes.  Never use `python time.sleep()` here: it
    # blocks the monitor thread that advances the simulation.
    if command -v setsid >/dev/null 2>&1; then
        setsid timeout "${GDB_EMULATOR_TIMEOUT:-300}" \
            bash -c 'tail -f /dev/null | exec "$@"' _ "${renode_cmd[@]}" \
            >"$BOARD_OUT/gdb-renode.log" 2>&1 </dev/null &
    else
        timeout "${GDB_EMULATOR_TIMEOUT:-300}" \
            bash -c 'tail -f /dev/null | exec "$@"' _ "${renode_cmd[@]}" \
            >"$BOARD_OUT/gdb-renode.log" 2>&1 </dev/null &
    fi
    emulator_pid=$!
fi

cleanup() {
    kill -TERM -- "-$emulator_pid" 2>/dev/null || kill "$emulator_pid" 2>/dev/null || true
    pkill -TERM -P "$emulator_pid" 2>/dev/null || true
    wait "$emulator_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Wait for the stub to listen (max ~10 s).  Do not probe the Renode port
# with a TCP connect: Renode treats the first connection as the debugger
# and may start emulation, which would run past early breakpoints before
# the real GDB attaches.
ready=0
for _ in $(seq 1 50); do
    if [ "$backend" = qemu ]; then
        if (exec 3<>"/dev/tcp/127.0.0.1/$PORT") 2>/dev/null; then
            exec 3>&- 2>/dev/null || true
            ready=1
            break
        fi
    elif grep -Fq -- "started on port :$PORT" "$BOARD_OUT/gdb-renode.log" 2>/dev/null; then
        ready=1
        break
    fi
    sleep 0.2
done
[ "$ready" -eq 1 ] || die "GDB stub did not listen on port $PORT"

info "GDB: $GDB_BIN (architecture $ARCH)"
gdb_args=()
if [ "${GDB_BATCH:-0}" = 1 ]; then
    gdb_args+=(-batch -ex "set pagination off" -ex "set confirm off")
fi
gdb_args+=(-ex "set architecture $arch_name" -ex "target remote 127.0.0.1:$PORT")
if [ "${GDB_BATCH:-0}" = 1 ]; then
    gdb_args+=(-ex "info registers pc" -ex "x/8i \$pc")
fi

if [ "${GDB_BATCH:-0}" = 1 ]; then
    info "Batch mode: connecting to 127.0.0.1:$PORT"
else
    info "Connecting to 127.0.0.1:$PORT; use Ctrl-D/Ctrl-C to leave."
fi
"$GDB_BIN" "$BIN" "${gdb_args[@]}" "$@"
