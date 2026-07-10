#!/usr/bin/env bash
# HAL version gate for CI.
#
# Fails when the file `crates/caplet-hal/src/version_gate.rs` has
# changed but neither HAL_VERSION_MAJOR nor HAL_VERSION_MINOR has
# been bumped. PATCH bumps do *not* need to accompany edits, but a
# MAJOR or MINOR bump *does* need an accompanying code change.
#
# Usage:   tools/hal_version_gate.sh
# Exit:    0 = all good, non-zero = fail.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FILE="$ROOT/crates/caplet-hal/src/version_gate.rs"

if [ ! -f "$FILE" ]; then
    echo "HAL version file not found: $FILE" >&2
    exit 1
fi

# 1. Compute current file hash and the one on the previous commit.
FILE_HASH_NOW="$(sha256sum "$FILE" | awk '{print $1}')"
FILE_HASH_HEAD="$(git -C "$ROOT" show HEAD:crates/caplet-hal/src/version_gate.rs 2>/dev/null \
    | sha256sum | awk '{print $1}' || echo "none")"

if [ "$FILE_HASH_NOW" = "$FILE_HASH_HEAD" ]; then
    echo "[hal-version-gate] no changes to version_gate.rs — ok"
    exit 0
fi

echo "[hal-version-gate] version_gate.rs changed"
echo "  before: $FILE_HASH_HEAD"
echo "  after:  $FILE_HASH_NOW"

# Pull MAJOR/MINOR from both states.
read_major_minor() {
    grep -E "HAL_VERSION_(MAJOR|MINOR): u16" "$1" \
        | awk '{ print $NF }' | tr '\n' '.' | sed 's/\.$//'
}

BEFORE_MM="$([ "$FILE_HASH_HEAD" != "none" ] \
    && { cd "$ROOT" && git show HEAD:crates/caplet-hal/src/version_gate.rs > /tmp/_vg_before.rs && read_major_minor /tmp/_vg_before.rs; } \
    || echo "0.0")"
AFTER_MM="$(read_major_minor "$FILE")"

echo "  major.minor before=$BEFORE_MM after=$AFTER_MM"

BEFORE_MAJOR="$(echo "$BEFORE_MM" | awk -F. '{print $1}')"
BEFORE_MINOR="$(echo "$BEFORE_MM" | awk -F. '{print $2}')"
AFTER_MAJOR="$(echo "$AFTER_MM" | awk -F. '{print $1}')"
AFTER_MINOR="$(echo "$AFTER_MM" | awk -F. '{print $2}')"

if [ -z "$AFTER_MAJOR" ] || [ -z "$AFTER_MINOR" ]; then
    echo "[hal-version-gate] cannot parse MAJOR/MINOR after — fail" >&2
    exit 1
fi

if [ "$AFTER_MAJOR" = "$BEFORE_MAJOR" ] && [ "$AFTER_MINOR" = "$BEFORE_MINOR" ]; then
    echo "[hal-version-gate] version_gate.rs changed but HAL_VERSION_MAJOR/MINOR did not bump." >&2
    echo "[hal-version-gate] bump at least HAL_VERSION_PATCH (or one of MAJOR/MINOR)." >&2
    exit 1
fi

echo "[hal-version-gate] OK (major.minor $BEFORE_MM -> $AFTER_MM)"
exit 0
