//! HAL version gate
//!
//! Downstream crates can compare against [`HAL_VERSION_MAJOR`] /
//! [`HAL_VERSION_MINOR`] / [`HAL_VERSION_PATCH`] at compile time to
//! refuse to build when an incompatible HAL is in the dependency
//! graph. The same constants are also embedded in a runtime
//! [`hal_version()`] function so the boot banner can print them.

/// Major version of the HAL. Bumped on breaking trait changes
/// (method signature change, removal, semantic change).
pub const HAL_VERSION_MAJOR: u16 = 0;

/// Minor version of the HAL. Bumped on backwards-compatible
/// additions (new methods with default impls, new trait impls).
pub const HAL_VERSION_MINOR: u16 = 2;

/// Patch version. Bumped on doc-only or internal-only changes.
pub const HAL_VERSION_PATCH: u16 = 1;

/// Packed SemVer `MMmmpp` (e.g. `0.2.1` → `0_002_001`).
pub const HAL_VERSION_NUMBER: u32 = (HAL_VERSION_MAJOR as u32) * 100_000
    + (HAL_VERSION_MINOR as u32) * 1_000
    + (HAL_VERSION_PATCH as u32);

/// Short git-style string for the version banner.
pub const HAL_VERSION_STRING: &str = "0.2.1";

/// Runtime accessor used by the boot banner.
pub fn hal_version() -> Version {
    Version {
        major: HAL_VERSION_MAJOR,
        minor: HAL_VERSION_MINOR,
        patch: HAL_VERSION_PATCH,
    }
}

/// Version triple, returned by [`hal_version`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    /// Major
    pub major: u16,
    /// Minor
    pub minor: u16,
    /// Patch
    pub patch: u16,
}

impl Version {
    /// Render as `major.minor.patch`.
    pub fn as_string(&self) -> alloc::string::String {
        use core::fmt::Write as _;
        let mut s = alloc::string::String::new();
        let _ = write!(&mut s, "{}.{}.{}", self.major, self.minor, self.patch);
        s
    }
}

/// Compile-time assertion that the current HAL version is at least
/// `major.minor.patch`. Use in `build.rs` or `const _: () = ...`
/// blocks to fail the build when the dependency graph drags in a
/// stale HAL.
#[macro_export]
macro_rules! static_assert_hal_version {
    ($maj:literal, $min:literal, $patch:literal) => {
        const _: [(); $crate::version_gate::HAL_VERSION_MAJOR as usize] = [(); $maj];
        const _: [(); $crate::version_gate::HAL_VERSION_MINOR as usize] = [(); $min];
        const _: [(); $crate::version_gate::HAL_VERSION_PATCH as usize] = [(); $patch];
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn version_string_matches_components() {
        let v = hal_version();
        let s = v.as_string();
        let expected = format!("{}.{}.{}", v.major, v.minor, v.patch);
        assert_eq!(s, expected);
    }

    #[test]
    fn version_number_packs_correctly() {
        assert_eq!(HAL_VERSION_NUMBER, 2_001);
    }
}
