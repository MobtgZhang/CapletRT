#!/usr/bin/env bash
# QEMU hello-nucleo-f446ze-qemu integration test.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_qemu.sh" nucleo-qemu
