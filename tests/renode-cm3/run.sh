#!/usr/bin/env bash
# Renode Cortex-M3 integration test.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_renode.sh cm3"
