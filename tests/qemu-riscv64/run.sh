#!/usr/bin/env bash
# QEMU RISC-V64 integration test.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_qemu.sh riscv64"
