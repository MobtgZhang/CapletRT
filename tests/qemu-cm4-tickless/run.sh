#!/usr/bin/env bash
# Compatibility entry point for the tickless-idle QEMU test.
#
# QEMU models the MPS2-AN386 Cortex-M4 machine, while the checked-in
# Renode platform for the same board is named mps2-an500.repl.  The
# test asserts runtime counters, not just the boot banner.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/tickless_verify.sh"
