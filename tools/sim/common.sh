#!/usr/bin/env bash
# Shared helpers for the CapletRT QEMU/Renode simulation matrix.
#
# This file is sourced by the other tools/sim scripts.  It deliberately
# uses only POSIX-ish bash features so it works on Linux CI and Git Bash.

set -uo pipefail

SIM_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SIM_DIR/../.." && pwd)"
SIM_OUT="$PROJECT_ROOT/target/sim"

die() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

info() {
    printf '%s\n' "$*"
}

find_program() {
    # find_program <environment-variable> <command...>
    local env_name="$1"
    shift
    local candidate
    candidate="${!env_name:-}"
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
        printf '%s\n' "$candidate"
        return 0
    fi
    for candidate in "$@"; do
        if command -v "$candidate" >/dev/null 2>&1; then
            command -v "$candidate"
            return 0
        fi
    done
    return 1
}

# board_spec <key>
#
# Emits a shell fragment consumed with eval.  Keeping the matrix in one
# place guarantees that QEMU, Renode and the GDB launcher agree on the
# package, target triple and expected boot marker for every board.
board_spec() {
    case "$1" in
        riscv32)
            cat <<'SPEC'
PACKAGE=hello-riscv
FEATURES=target-none,rv32imac
TARGET=riscv32imac-unknown-none-elf
ARCH=riscv32
QEMU_VAR=QEMU_SYSTEM_RISCV32
QEMU_NAMES=qemu-system-riscv32
REPL=boards/hello-riscv/renode/rv32.repl
RESC=boards/hello-riscv/renode/riscv-virt.resc
EXPECT=CapletRT\ RISC-V\ boot\ OK
SPEC
            ;;
        riscv64)
            cat <<'SPEC'
PACKAGE=hello-riscv
FEATURES=target-none,rv64gc
TARGET=riscv64gc-unknown-none-elf
ARCH=riscv64
QEMU_VAR=QEMU_SYSTEM_RISCV64
QEMU_NAMES=qemu-system-riscv64
REPL=boards/hello-riscv/renode/rv64.repl
RESC=boards/hello-riscv/renode/riscv-virt.resc
EXPECT=CapletRT\ RISC-V\ boot\ OK
SPEC
            ;;
        aarch64|aarch64-qemu)
            cat <<'SPEC'
PACKAGE=hello-aarch64-qemu
FEATURES=target-none
TARGET=aarch64-unknown-none
ARCH=aarch64
QEMU_VAR=QEMU_SYSTEM_AARCH64
QEMU_NAMES=qemu-system-aarch64
REPL=boards/hello-aarch64-qemu/renode/cortex-a53-gicv3.repl
RESC=boards/hello-aarch64-qemu/renode/cortex-a53.resc
EXPECT=CapletRT\ AArch64\ boot\ OK
SPEC
            ;;
        cortex-a9)
            cat <<'SPEC'
PACKAGE=hello-cortex-a9
FEATURES=target-none
TARGET=armv7a-none-eabihf
ARCH=armv7a-a9
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/hello-cortex-a9/renode/cortex-a9.repl
RESC=boards/hello-cortex-a9/renode/cortex-a9.resc
EXPECT=CapletRT\ Cortex-A9\ boot\ OK
SPEC
            ;;
        a20|allwinner-a20)
            cat <<'SPEC'
PACKAGE=hello-allwinner-a20-qemu
FEATURES=target-none
TARGET=armv7a-none-eabihf
ARCH=armv7a-a20
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/hello-allwinner-a20-qemu/renode/allwinner-a20.repl
RESC=boards/hello-allwinner-a20-qemu/renode/allwinner-a20.resc
EXPECT=CapletRT\ Allwinner\ A20\ boot\ OK
SPEC
            ;;
        cm3)
            cat <<'SPEC'
PACKAGE=hello-qemu-cm3
FEATURES=target-none
TARGET=thumbv7m-none-eabi
ARCH=armv7m
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/hello-qemu-cm3/renode/mps2-an385.repl
RESC=boards/hello-qemu-cm3/renode/mps2-an385.resc
EXPECT=hello\ world
SPEC
            ;;
        cm4)
            cat <<'SPEC'
PACKAGE=hello-qemu-cm4
FEATURES=target-none
TARGET=thumbv7em-none-eabihf
ARCH=armv7em
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/hello-qemu-cm4/renode/mps2-an500.repl
RESC=boards/hello-qemu-cm4/renode/mps2-an500.resc
EXPECT=hello\ world
SPEC
            ;;
        cm4-fs)
            cat <<'SPEC'
PACKAGE=hello-qemu-cm4-fs
FEATURES=target-none
TARGET=thumbv7em-none-eabihf
ARCH=armv7em
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/hello-qemu-cm4-fs/renode/mps2-an500.repl
RESC=boards/hello-qemu-cm4-fs/renode/stm32f405.resc
EXPECT=VFS\ demo\ task\ done
SPEC
            ;;
        nucleo-qemu)
            cat <<'SPEC'
PACKAGE=hello-nucleo-f446ze-qemu
FEATURES=target-none
TARGET=thumbv7m-none-eabi
ARCH=stm32f4
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/hello-nucleo-f446ze-qemu/renode/stm32f446ze.repl
RESC=boards/hello-nucleo-f446ze-qemu/renode/stm32f446ze.resc
EXPECT=Nucleo-F446ZE\ boot\ OK
SPEC
            ;;
        nucleo)
            cat <<'SPEC'
PACKAGE=nucleo-f446ze
FEATURES=target-none
TARGET=thumbv7em-none-eabihf
ARCH=stm32f4
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/nucleo-f446ze/renode/nucleo-f446ze.repl
RESC=boards/nucleo-f446ze/renode/nucleo-f446ze.resc
EXPECT=Nucleo-F446ZE\ boot\ OK
SPEC
            ;;
        industrial-demo)
            cat <<'SPEC'
PACKAGE=industrial-demo
FEATURES=target-none
TARGET=thumbv7em-none-eabihf
ARCH=armv7em
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/industrial-demo/renode/mps2-an500.repl
RESC=boards/industrial-demo/renode/mps2-an500.resc
EXPECT=CapletRT\ M4\ industrial\ scenario
SPEC
            ;;
        energy-meter)
            cat <<'SPEC'
PACKAGE=energy-meter
FEATURES=target-none
TARGET=thumbv7em-none-eabihf
ARCH=armv7em
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/energy-meter/renode/mps2-an500.repl
RESC=boards/energy-meter/renode/mps2-an500.resc
EXPECT=energy-meter:\ CapletRT\ M4\ industrial\ scenario
SPEC
            ;;
        medical-device)
            cat <<'SPEC'
PACKAGE=medical-device
FEATURES=target-none
TARGET=thumbv7em-none-eabihf
ARCH=armv7em
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/medical-device/renode/mps2-an500.repl
RESC=boards/medical-device/renode/mps2-an500.resc
EXPECT=medical-device:\ CapletRT\ M4\ safety-critical\ scenario
SPEC
            ;;
        automotive)
            cat <<'SPEC'
PACKAGE=automotive
FEATURES=target-none
TARGET=thumbv7em-none-eabihf
ARCH=armv7em
QEMU_VAR=QEMU_SYSTEM_ARM
QEMU_NAMES=qemu-system-arm
REPL=boards/automotive/renode/mps2-an500.repl
RESC=boards/automotive/renode/mps2-an500.resc
EXPECT=automotive:\ CapletRT\ M4\ automotive\ scenario
SPEC
            ;;
        industrial-demo-riscv32)
            cat <<'SPEC'
PACKAGE=industrial-demo
FEATURES=target-none,rv32imac
TARGET=riscv32imac-unknown-none-elf
ARCH=riscv32
QEMU_VAR=QEMU_SYSTEM_RISCV32
QEMU_NAMES=qemu-system-riscv32
REPL=boards/industrial-demo/renode/riscv32.repl
RESC=boards/industrial-demo/renode/riscv-virt.resc
EXPECT=CapletRT\ M4\ industrial\ scenario
SPEC
            ;;
        industrial-demo-riscv64)
            cat <<'SPEC'
PACKAGE=industrial-demo
FEATURES=target-none,rv64gc
TARGET=riscv64gc-unknown-none-elf
ARCH=riscv64
QEMU_VAR=QEMU_SYSTEM_RISCV64
QEMU_NAMES=qemu-system-riscv64
REPL=boards/industrial-demo/renode/riscv64.repl
RESC=boards/industrial-demo/renode/riscv-virt.resc
EXPECT=CapletRT\ M4\ industrial\ scenario
SPEC
            ;;
        energy-meter-riscv32)
            cat <<'SPEC'
PACKAGE=energy-meter
FEATURES=target-none,rv32imac
TARGET=riscv32imac-unknown-none-elf
ARCH=riscv32
QEMU_VAR=QEMU_SYSTEM_RISCV32
QEMU_NAMES=qemu-system-riscv32
REPL=boards/energy-meter/renode/riscv32.repl
RESC=boards/energy-meter/renode/riscv-virt.resc
EXPECT=energy-meter:\ CapletRT\ M4\ industrial\ scenario
SPEC
            ;;
        energy-meter-riscv64)
            cat <<'SPEC'
PACKAGE=energy-meter
FEATURES=target-none,rv64gc
TARGET=riscv64gc-unknown-none-elf
ARCH=riscv64
QEMU_VAR=QEMU_SYSTEM_RISCV64
QEMU_NAMES=qemu-system-riscv64
REPL=boards/energy-meter/renode/riscv64.repl
RESC=boards/energy-meter/renode/riscv-virt.resc
EXPECT=energy-meter:\ CapletRT\ M4\ industrial\ scenario
SPEC
            ;;
        medical-device-riscv32)
            cat <<'SPEC'
PACKAGE=medical-device
FEATURES=target-none,rv32imac
TARGET=riscv32imac-unknown-none-elf
ARCH=riscv32
QEMU_VAR=QEMU_SYSTEM_RISCV32
QEMU_NAMES=qemu-system-riscv32
REPL=boards/medical-device/renode/riscv32.repl
RESC=boards/medical-device/renode/riscv-virt.resc
EXPECT=medical-device:\ CapletRT\ M4\ safety-critical\ scenario
SPEC
            ;;
        medical-device-riscv64)
            cat <<'SPEC'
PACKAGE=medical-device
FEATURES=target-none,rv64gc
TARGET=riscv64gc-unknown-none-elf
ARCH=riscv64
QEMU_VAR=QEMU_SYSTEM_RISCV64
QEMU_NAMES=qemu-system-riscv64
REPL=boards/medical-device/renode/riscv64.repl
RESC=boards/medical-device/renode/riscv-virt.resc
EXPECT=medical-device:\ CapletRT\ M4\ safety-critical\ scenario
SPEC
            ;;
        automotive-riscv32)
            cat <<'SPEC'
PACKAGE=automotive
FEATURES=target-none,rv32imac
TARGET=riscv32imac-unknown-none-elf
ARCH=riscv32
QEMU_VAR=QEMU_SYSTEM_RISCV32
QEMU_NAMES=qemu-system-riscv32
REPL=boards/automotive/renode/riscv32.repl
RESC=boards/automotive/renode/riscv-virt.resc
EXPECT=automotive:\ CapletRT\ M4\ automotive\ scenario
SPEC
            ;;
        automotive-riscv64)
            cat <<'SPEC'
PACKAGE=automotive
FEATURES=target-none,rv64gc
TARGET=riscv64gc-unknown-none-elf
ARCH=riscv64
QEMU_VAR=QEMU_SYSTEM_RISCV64
QEMU_NAMES=qemu-system-riscv64
REPL=boards/automotive/renode/riscv64.repl
RESC=boards/automotive/renode/riscv-virt.resc
EXPECT=automotive:\ CapletRT\ M4\ automotive\ scenario
SPEC
            ;;
        *)
            return 1
            ;;
    esac
}

load_board() {
    local key="$1"
    local fragment
    fragment="$(board_spec "$key")" || {
        printf 'FAIL: unknown board key %s\n' "$key" >&2
        printf 'known boards: %s\n' "$(all_boards | tr '\n' ' ')" >&2
        exit 2
    }
    eval "$fragment"
    BOARD_KEY="$key"
    BIN="$PROJECT_ROOT/target/$TARGET/release/$PACKAGE"
    BOARD_OUT="$SIM_OUT/$key"
}

# qemu_board_args
#
# Populate QEMU_ARGS from the selected board architecture.  Sharing this
# function between run_qemu.sh and gdb_debug.sh prevents the debug launcher
# from silently drifting to a different machine model.
qemu_board_args() {
    case "$ARCH" in
        riscv32)
            QEMU_ARGS=(-machine virt -cpu rv32 -nographic -m 128M -bios none -kernel "$BIN")
            ;;
        riscv64)
            QEMU_ARGS=(-machine virt -cpu rv64 -nographic -m 128M -bios none -kernel "$BIN")
            ;;
        aarch64)
            QEMU_ARGS=(-machine virt -cpu cortex-a53 -nographic -m 128M -kernel "$BIN")
            ;;
        armv7a-a9)
            QEMU_ARGS=(-machine vexpress-a9 -cpu cortex-a9 -m 256M -nographic \
                       -monitor none -semihosting-config enable=on,target=native -kernel "$BIN")
            ;;
        armv7a-a20)
            QEMU_ARGS=(-machine cubieboard -cpu cortex-a8 -m 1G -nographic \
                       -monitor none -kernel "$BIN")
            ;;
        armv7m)
            QEMU_ARGS=(-machine mps2-an385 -cpu cortex-m3 -nographic \
                       -semihosting-config enable=on,target=native -kernel "$BIN")
            ;;
        armv7em)
            QEMU_ARGS=(-machine mps2-an386 -cpu cortex-m4 -nographic \
                       -semihosting-config enable=on,target=native -kernel "$BIN")
            ;;
        stm32f4)
            QEMU_ARGS=(-machine olimex-stm32-h405 -nographic \
                       -semihosting-config enable=on,target=native -kernel "$BIN")
            ;;
        *)
            die "no QEMU arguments for architecture $ARCH"
            ;;
    esac

    if [ -n "${QEMU_SMP:-}" ]; then
        case "$ARCH" in
            armv7a-a9|armv7a-a20|aarch64)
                QEMU_ARGS+=(-smp "$QEMU_SMP")
                ;;
            *)
                die "QEMU_SMP is not supported for $ARCH"
                ;;
        esac
    fi
}

all_boards() {
    printf '%s\n' \
        riscv32 riscv64 aarch64 cortex-a9 a20 \
        cm3 cm4 cm4-fs nucleo-qemu nucleo \
        industrial-demo energy-meter medical-device automotive \
        industrial-demo-riscv32 industrial-demo-riscv64 \
        energy-meter-riscv32 energy-meter-riscv64 \
        medical-device-riscv32 medical-device-riscv64 \
        automotive-riscv32 automotive-riscv64
}

build_board() {
    local profile="${1:-release}"
    local profile_dir=debug
    local cargo_args=()
    [ "$profile" = release ] && profile_dir=release
    if [ "$profile" = release ]; then
        cargo_args=(--release)
    else
        cargo_args=(--profile "$profile")
    fi
    mkdir -p "$SIM_OUT"
    local build_log="$SIM_OUT/build-$PACKAGE-$TARGET-$profile.log"
    info "Building $PACKAGE ($TARGET${FEATURES:+, features=$FEATURES}, profile=$profile)..."
    # shellcheck disable=SC2086
    if ! ( cd "$PROJECT_ROOT" && cargo build --quiet "${cargo_args[@]}" -p "$PACKAGE" \
        --features "$FEATURES" --target "$TARGET" ) >"$build_log" 2>&1; then
        printf '%s\n' '--- build output ---' >&2
        cat "$build_log" >&2
        die "build failed for $PACKAGE"
    fi
    BIN="$PROJECT_ROOT/target/$TARGET/$profile_dir/$PACKAGE"
    [ -f "$BIN" ] || die "expected ELF not found: $BIN"
}

assert_log_contains() {
    local log="$1"
    local pattern="$2"
    if grep -Fq -- "$pattern" "$log"; then
        return 0
    fi
    printf 'FAIL: expected %s in %s\n' "$pattern" "$log" >&2
    printf '%s\n' '--- captured output (first 200 lines) ---' >&2
    head -200 "$log" >&2 || true
    printf '%s\n' '--- end output ---' >&2
    return 1
}

assert_log_absent() {
    local log="$1"
    local pattern="$2"
    if grep -Fq -- "$pattern" "$log"; then
        printf 'FAIL: unexpected %s in %s\n' "$pattern" "$log" >&2
        printf '%s\n' '--- captured output (first 200 lines) ---' >&2
        head -200 "$log" >&2 || true
        printf '%s\n' '--- end output ---' >&2
        return 1
    fi
}

# assert_board_log <log>
#
# Every board must reach its documented functional completion marker, not
# merely print a banner.  Industry demos additionally get exact assertions
# for storage, watchdog, alerting and MQTT-SN acknowledgements.
assert_board_log() {
    local log="$1"
    local failed=0

    assert_log_contains "$log" "$EXPECT" || failed=1

    case "$BOARD_KEY" in
        riscv32|riscv64)
            assert_log_contains "$log" "kernel: starting cooperative scheduler" || failed=1
            assert_log_contains "$log" "CapletRT RISC-V kernel scheduler OK" || failed=1
            assert_log_contains "$log" "kernel: task A done" || failed=1
            assert_log_contains "$log" "kernel: task B done" || failed=1
            ;;
        aarch64)
            assert_log_contains "$log" "kernel: starting cooperative scheduler" || failed=1
            assert_log_contains "$log" "CapletRT AArch64 kernel scheduler OK" || failed=1
            assert_log_contains "$log" "kernel: task A done" || failed=1
            assert_log_contains "$log" "kernel: task B done" || failed=1
            ;;
        cortex-a9)
            assert_log_contains "$log" "kernel: starting cooperative scheduler" || failed=1
            assert_log_contains "$log" "CapletRT Cortex-A9 kernel scheduler OK" || failed=1
            assert_log_contains "$log" "kernel: task A done" || failed=1
            assert_log_contains "$log" "kernel: task B done" || failed=1
            ;;
        a20)
            assert_log_contains "$log" "kernel: starting cooperative scheduler" || failed=1
            assert_log_contains "$log" "CapletRT Allwinner A20 kernel scheduler OK" || failed=1
            assert_log_contains "$log" "kernel: task A done" || failed=1
            assert_log_contains "$log" "kernel: task B done" || failed=1
            ;;
        cm3)
            assert_log_contains "$log" "cap_init: done. Boot task running." || failed=1
            assert_log_contains "$log" "demo: hello world" || failed=1
            assert_log_contains "$log" "control: tick 1000" || failed=1
            ;;
        cm4)
            assert_log_contains "$log" "task_table: linked=3 explicit=3 launched=3 skipped=0" || failed=1
            assert_log_contains "$log" "demo: hello world" || failed=1
            assert_log_contains "$log" "control: tick 1000" || failed=1
            ;;
        cm4-fs)
            assert_log_contains "$log" "fs: verified 29 bytes after close/reopen" || failed=1
            assert_log_contains "$log" "LittleFS mount='data' verified 13 bytes" || failed=1
            ;;
        nucleo-qemu|nucleo)
            assert_log_contains "$log" "task_table: linked=0 explicit=2 launched=2 skipped=0" || failed=1
            assert_log_contains "$log" "task led done" || failed=1
            assert_log_contains "$log" "task monitor done" || failed=1
            assert_log_contains "$log" "Nucleo-F446ZE kernel scheduler OK" || failed=1
            ;;
        industrial-demo|industrial-demo-riscv32|industrial-demo-riscv64)
            assert_log_contains "$log" "PUBLISH topic=sensors/temp qos=1" || failed=1
            assert_log_contains "$log" "UPLOAD complete=5 packets=9" || failed=1
            assert_log_contains "$log" "STORAGE persisted" || failed=1
            assert_log_contains "$log" "WATCHDOG healthy" || failed=1
            ;;
        energy-meter|energy-meter-riscv32|energy-meter-riscv64)
            assert_log_contains "$log" "PUBLISH topic=energy/meter qos=1" || failed=1
            assert_log_contains "$log" "UPLOAD complete=5 packets=9" || failed=1
            assert_log_contains "$log" "STORAGE persisted batch=20 records=20 total=20" || failed=1
            ;;
        medical-device|medical-device-riscv32|medical-device-riscv64)
            assert_log_contains "$log" "ALARM FIRED: count=1" || failed=1
            assert_log_contains "$log" "alarm capability revoked" || failed=1
            assert_log_contains "$log" "MEDICAL complete alarm=1" || failed=1
            assert_log_contains "$log" "dual=ok" || failed=1
            ;;
        automotive|automotive-riscv32|automotive-riscv64)
            assert_log_contains "$log" "blackbox: record #0 written: id=0x100" || failed=1
            assert_log_contains "$log" "blackbox: record #1 written: id=0x200" || failed=1
            assert_log_contains "$log" "blackbox: record #2 written: id=0x400" || failed=1
            assert_log_contains "$log" "PUBLISH topic=automotive/fault qos=1" || failed=1
            assert_log_contains "$log" "ALERTS complete=3" || failed=1
            assert_log_contains "$log" "AUTOMOTIVE complete faults=3 alerts=3 blackbox=3" || failed=1
            ;;
    esac

    assert_log_absent "$log" "ERROR:" || failed=1
    assert_log_absent "$log" "FATAL:" || failed=1
    assert_log_absent "$log" "WARNING: no storage writes or uploads" || failed=1
    return "$failed"
}

print_log_excerpt() {
    local log="$1"
    local lines="${2:-60}"
    printf '%s\n' "--- $(basename "$log") ($(wc -c < "$log" | tr -d ' ') bytes, first $lines lines) ---"
    head -"$lines" "$log" || true
    printf '%s\n' '--- end excerpt ---'
}
