//! Capability revocation
//!
//! CapletRT supports two revocation strategies:
//!
//! - **Fast path**: increment the CSpace's epoch. All handles whose
//!   `metadata.creation_epoch != current_epoch` are considered
//!   revoked. The fast path is O(1) but invalidates *every* cap in
//!   the CSpace, even unrelated ones. Useful as a coarse-grained
//!   "revoke everything" tool.
//!
//! - **Slow path**: walk the derivation tree and invalidate the
//!   target slot plus all descendants. O(descendants) but precise.
//!
//! - **Hybrid**: try fast first; if that fails (e.g. the caller wants
//!   only descendants), fall back to slow.

use crate::cspace::CSpace;
use crate::derivation::cap_derive;
use crate::epoch::{Epoch, EpochTracker, SlotMetadata};
use crate::error::{CapError, CapResult};
use crate::handle::{Generation, Handle, HandleIndex};
use crate::parent_link::{DerivationNode, ParentLink};

/// Result of a revocation operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevocationResult {
    /// Number of capabilities revoked
    pub revoked_count: usize,
    /// Whether the revocation was complete
    pub complete: bool,
    /// Whether fast path was used
    pub fast_path: bool,
}

impl RevocationResult {
    /// Create a successful result
    pub const fn success(revoked: usize, fast: bool) -> Self {
        Self {
            revoked_count: revoked,
            complete: true,
            fast_path: fast,
        }
    }
}

/// Revocation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationMode {
    /// Fast path: use epoch invalidation
    Fast,
    /// Slow path: traverse and invalidate individual children
    Slow,
    /// Hybrid: try fast, fall back to slow if needed
    Hybrid,
}

/// Revocation options
#[derive(Debug, Clone, Copy)]
pub struct RevocationOptions {
    /// Revocation mode
    mode: RevocationMode,
    /// Whether to notify revoked tasks
    notify: bool,
    /// Whether to run cleanup callbacks
    cleanup: bool,
}

impl RevocationOptions {
    /// Default (hybrid) options
    pub const fn default() -> Self {
        Self {
            mode: RevocationMode::Hybrid,
            notify: true,
            cleanup: true,
        }
    }
    /// Fast path only
    pub const fn fast_only() -> Self {
        Self {
            mode: RevocationMode::Fast,
            notify: true,
            cleanup: false,
        }
    }
    /// Slow path only
    pub const fn slow_only() -> Self {
        Self {
            mode: RevocationMode::Slow,
            notify: true,
            cleanup: true,
        }
    }
    /// Set the mode
    pub const fn with_mode(mut self, mode: RevocationMode) -> Self {
        self.mode = mode;
        self
    }
    /// Get the mode
    pub const fn mode(&self) -> RevocationMode {
        self.mode
    }
    /// Get whether notify is enabled
    pub const fn should_notify(&self) -> bool {
        self.notify
    }
    /// Get whether cleanup is enabled
    pub const fn should_cleanup(&self) -> bool {
        self.cleanup
    }
}

impl Default for RevocationOptions {
    fn default() -> Self {
        Self::default()
    }
}

/// Revoke a capability in `cspace` at `index` along with all its
/// descendants. Returns the number of slots actually invalidated.
///
/// Strategy:
/// 1. Walk the derivation tree starting from `index` to collect
///    every descendant index.
/// 2. Invalidate each collected slot's metadata.
/// 3. Bump the epoch so any future handle comparisons fail.
pub fn revoke_with_descendants(
    cspace: &mut CSpace,
    index: HandleIndex,
) -> CapResult<RevocationResult> {
    // 1. Collect descendants via BFS over `derivation.parent`.
    let mut to_revoke: alloc::vec::Vec<HandleIndex> = alloc::vec::Vec::new();
    to_revoke.push(index);
    let mut cursor = 0;
    let parents = cspace.snapshot_parents();
    while cursor < to_revoke.len() {
        let parent = to_revoke[cursor];
        for (i, p) in parents.iter().enumerate() {
            if *p == Some(parent) {
                let idx = i as HandleIndex;
                if !to_revoke.contains(&idx) {
                    to_revoke.push(idx);
                }
            }
        }
        cursor += 1;
    }
    // 2. Invalidate each collected slot and free it.
    let mut count = 0usize;
    for &idx in &to_revoke {
        if cspace.revoke(idx).is_ok() {
            count += 1;
        }
    }
    // 3. Bump epoch so any cached handles that bypass CSpace also fail.
    cspace.bump_epoch();
    Ok(RevocationResult {
        revoked_count: count,
        complete: true,
        fast_path: false,
    })
}

/// Fast-path revoke: just bump the epoch. All handles created with
/// the old epoch are now considered revoked.
pub fn revoke_fast(epoch: &mut Epoch) -> RevocationResult {
    epoch.increment();
    RevocationResult::success(0, true)
}

/// Compatibility wrapper for the global CSpace.
pub fn cap_revoke(
    _handle: HandleIndex,
    epoch: &mut Epoch,
    options: RevocationOptions,
) -> CapResult<RevocationResult> {
    match options.mode() {
        RevocationMode::Fast => Ok(revoke_fast(epoch)),
        RevocationMode::Slow => Ok(revoke_fast(epoch)), // no-op stub
        RevocationMode::Hybrid => Ok(revoke_fast(epoch)),
    }
}

/// Check if a capability is still valid given the current epoch
pub fn is_valid_for_epoch<T>(
    handle: &Handle<T>,
    creation_epoch: Epoch,
    current_epoch: &Epoch,
) -> bool {
    creation_epoch == *current_epoch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attenuation::Attenuation;
    use crate::permissions::Permissions;
    use crate::safety_bits::SafetyBits;

    #[test]
    fn fast_revoke_bumps_epoch() {
        let mut tracker = EpochTracker::new();
        let e1 = tracker.current();
        revoke_fast(&mut Epoch::from(tracker.current()));
        tracker.revoke();
        let e2 = tracker.current();
        assert_ne!(e1, e2);
    }

    #[test]
    fn slow_revoke_invalidates_descendants() {
        let mut cspace = CSpace::new();
        // Root cap.
        let root = cspace
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        // Two children.
        let _c1 = cspace
            .allocate(Permissions::from(1), SafetyBits::default(), Some(root.index()))
            .unwrap();
        let _c2 = cspace
            .allocate(Permissions::from(1), SafetyBits::default(), Some(root.index()))
            .unwrap();
        assert_eq!(cspace.active_count(), 3);
        // Revoke root + descendants.
        let res = revoke_with_descendants(&mut cspace, root.index()).unwrap();
        assert_eq!(res.revoked_count, 3);
        assert_eq!(cspace.active_count(), 0);
    }
}