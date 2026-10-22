#!/usr/bin/env bash
# Cortex-A9 SMP bring-up test.
#
# QEMU releases all configured vexpress-a9 CPUs at the same reset
# vector. The board must release each secondary through the spin-table
# mailbox, install its private stack and the shared MMU map, and print
# one online record per CPU, then execute one observable worker task per
# secondary. Both the online count and the worker count must match
# QEMU's `-smp` value exactly.

set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

for cpus in 2 4; do
    echo "==> vexpress-a9 with -smp $cpus"
    QEMU_SMP="$cpus" "$ROOT/tools/sim/run_qemu.sh" cortex-a9
    log="$ROOT/target/sim/cortex-a9/qemu.log"

    grep -Fq "smp: online cpus=$cpus" "$log" || {
        echo "FAIL: guest did not observe $cpus online CPUs" >&2
        exit 1
    }
    secondaries=$((cpus - 1))
    grep -Fq "smp: worker tasks done=$secondaries" "$log" || {
        echo "FAIL: $secondaries secondary worker task(s) did not complete" >&2
        exit 1
    }

    case "$cpus" in
        2) expected="1" ;;
        4) expected="1 2 3" ;;
        *) echo "FAIL: unsupported test CPU count $cpus" >&2; exit 1 ;;
    esac
    for cpu in $expected; do
        grep -Fq "smp: cpu$cpu online" "$log" || {
            echo "FAIL: secondary cpu$cpu did not reach its Rust entry" >&2
            exit 1
        }
        grep -Fq "smp: cpu$cpu worker done" "$log" || {
            echo "FAIL: secondary cpu$cpu did not execute its worker task" >&2
            exit 1
        }
    done
done

echo "PASS: Cortex-A9 SMP (2- and 4-core online + observable worker tasks)"
