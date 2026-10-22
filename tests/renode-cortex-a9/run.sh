#!/usr/bin/env bash
# Renode Cortex-A9 integration test.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_renode.sh cortex-a9"
