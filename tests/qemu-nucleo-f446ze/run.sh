#!/usr/bin/env bash
# QEMU STM32F446ZE (Nucleo-F446ZE) integration test.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_qemu.sh" nucleo
