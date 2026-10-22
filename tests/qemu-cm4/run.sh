#!/usr/bin/env bash
# QEMU Cortex-M4 integration test (MPS2-AN500).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_qemu.sh" cm4
