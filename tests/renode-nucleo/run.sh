#!/usr/bin/env bash
# Renode Nucleo-F446ZE integration test.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_renode.sh nucleo-qemu"
