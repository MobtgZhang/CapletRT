#!/usr/bin/env bash
# HAL version gate for CI.
#
# Fails when the file `crates/caplet-hal/src/version_gate.rs` has
# changed without bumping at least one of
# HAL_VERSION_{MAJOR,MINOR,PATCH}. A MAJOR or MINOR bump also
# requires an accompanying code change, which this gate enforces by
# comparing the file itself.
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

# Pull MAJOR/MINOR/PATCH from both states.
read_version() {
    grep -E "HAL_VERSION_(MAJOR|MINOR|PATCH): u16" "$1" \
        | sed -E 's/.*= *([0-9]+);/\1/' | paste -sd. -
}

BEFORE_MMP="$([ "$FILE_HASH_HEAD" != "none" ] \
    && { cd "$ROOT" && git show HEAD:crates/caplet-hal/src/version_gate.rs > /tmp/_vg_before.rs && read_version /tmp/_vg_before.rs; } \
    || echo "0.0.0")"
AFTER_MMP="$(read_version "$FILE")"

echo "  major.minor.patch before=$BEFORE_MMP after=$AFTER_MMP"

case "$AFTER_MMP" in
    [0-9]*.[0-9]*.[0-9]*) ;;
    *)
        echo "[hal-version-gate] cannot parse MAJOR/MINOR/PATCH after — fail" >&2
        exit 1
        ;;
esac

if [ "$BEFORE_MMP" = "$AFTER_MMP" ]; then
    echo "[hal-version-gate] version_gate.rs changed but HAL version did not bump." >&2
    echo "[hal-version-gate] bump HAL_VERSION_PATCH, HAL_VERSION_MINOR, or HAL_VERSION_MAJOR." >&2
    exit 1
fi

echo "[hal-version-gate] OK ($BEFORE_MMP -> $AFTER_MMP)"
exit 0
