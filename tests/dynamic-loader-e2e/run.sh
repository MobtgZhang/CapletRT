#!/usr/bin/env bash
# End-to-end test for the CapletRT loader and ELF-to-.capp pipeline.
#
# Covers the built-in security checks plus a real external C ELF converted
# by tools/elf2capp.py, an out-of-image entry point rejection and a malformed
# container rejection.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="$ROOT/target/dynamic-loader-e2e"
CC="${CC:-clang}"
BIN="$ROOT/target/release/hello-dynamic-loader"

mkdir -p "$OUT"

fail() {
    echo "[dynamic-loader-e2e] FAIL: $*" >&2
    exit 1
}

contains() {
    local log="$1"
    local text="$2"
    grep -Fq -- "$text" "$log" || fail "missing '$text' in $log"
}

command -v "$CC" >/dev/null 2>&1 || fail "C compiler not found: $CC"
command -v python3 >/dev/null 2>&1 || fail "python3 not found"

echo "[dynamic-loader-e2e] running built-in loader checks..."
(
    cd "$ROOT"
    cargo build --quiet -p hello-dynamic-loader --release
) >"$OUT/builtin.log" 2>"$OUT/builtin.stderr"
"$BIN" >"$OUT/builtin.log" 2>"$OUT/builtin.stderr"

contains "$OUT/builtin.log" "unsigned module rejected by Enforce policy"
contains "$OUT/builtin.log" "executed entry -> 42"
contains "$OUT/builtin.log" "out-of-image entry rejected: BadEntry"
contains "$OUT/builtin.log" "signed module verified and executed -> 42"
contains "$OUT/builtin.log" "tampered signed module rejected: BadSignature"
contains "$OUT/builtin.log" "hello-dynamic-loader: PASS"

printf '%s\n' \
    '__attribute__((visibility("default"))) int module_entry(void) { return 42; }' \
    >"$OUT/module.c"

echo "[dynamic-loader-e2e] compiling real C module..."
"$CC" -O2 -ffreestanding -fno-stack-protector -fno-pie -no-pie -nostdlib \
    -Wl,--build-id=none -Wl,-e,module_entry -Wl,-Ttext=0x60000000 \
    -o "$OUT/module.elf" "$OUT/module.c"

echo "[dynamic-loader-e2e] converting ELF to relocatable .capp..."
python3 "$ROOT/tools/elf2capp.py" "$OUT/module.elf" \
    -o "$OUT/module.capp" --name module --cap log:3 >"$OUT/elf2capp.log"
contains "$OUT/elf2capp.log" "relocatable=True"

echo "[dynamic-loader-e2e] executing external .capp..."
(
    "$BIN" --load "$OUT/module.capp"
) >"$OUT/external.log" 2>"$OUT/external.stderr"
contains "$OUT/external.log" "entry: 0x1000"
contains "$OUT/external.log" "granted_caps: 1"
contains "$OUT/external.log" "executed -> 42"

cp "$OUT/module.capp" "$OUT/bad-entry.capp"
python3 - "$OUT/bad-entry.capp" <<'PY'
import struct
import sys

path = sys.argv[1]
data = bytearray(open(path, "rb").read())
struct.pack_into("<I", data, 8, 0x100000)
open(path, "wb").write(data)
PY

echo "[dynamic-loader-e2e] checking out-of-image entry rejection..."
if (
    "$BIN" --load "$OUT/bad-entry.capp"
) >"$OUT/bad-entry.log" 2>"$OUT/bad-entry.stderr"; then
    fail "loader accepted an out-of-image entry"
fi
grep -Fq -- "BadEntry" "$OUT/bad-entry.stderr" || \
    fail "out-of-image module did not report BadEntry"

cp "$OUT/module.capp" "$OUT/bad-magic.capp"
printf 'X' | dd of="$OUT/bad-magic.capp" bs=1 seek=0 conv=notrunc status=none

echo "[dynamic-loader-e2e] checking malformed container rejection..."
if (
    "$BIN" --load "$OUT/bad-magic.capp"
) >"$OUT/bad-magic.log" 2>"$OUT/bad-magic.stderr"; then
    fail "loader accepted a malformed .capp container"
fi
grep -Fq -- "BadMagic" "$OUT/bad-magic.stderr" || \
    fail "malformed module did not report BadMagic"

echo "[dynamic-loader-e2e] PASS: built-in policies and external ELF/.capp load verified"
