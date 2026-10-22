#!/usr/bin/env bash
# Renode Cortex-M4 VFS integration test.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_renode.sh cm4-fs"
