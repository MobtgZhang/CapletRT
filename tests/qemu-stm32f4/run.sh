#!/usr/bin/env bash
# Small compatibility entry point for the historical QEMU STM32F4 test.
#
# The board matrix is the source of truth.  `nucleo-qemu` builds the
# `hello-nucleo-f446ze-qemu` image for thumbv7m and runs it on QEMU's
# `olimex-stm32-h405` machine, which is the STM32F405/F446-compatible model.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SIM_TIMEOUT="${SIM_TIMEOUT:-10}" "$ROOT/tools/sim/run_qemu.sh" nucleo-qemu

log="$ROOT/target/sim/nucleo-qemu/qemu.log"
grep -Fq "Nucleo-F446ZE boot OK" "$log"
echo "PASS: QEMU STM32F4 (nucleo-qemu)"
