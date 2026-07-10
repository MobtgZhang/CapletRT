//! Mount table
//!
//! Maps the first path segment of every absolute path to a
//! [`MountPoint`] (a name + driver). The VFS resolves
//! `/<mount>/<sub>` by looking up `<mount>` in the table and
//! forwarding the call to the driver.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;

use crate::driver::FsDriver;
use crate::path::Path;

/// Unique mount ID. Issued by [`MountTable::mount`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MountId(pub u32);

/// Mount point — a name + a reference to the driver.
pub struct MountPoint {
    /// Mount name, e.g. `"data"`, `"tmp"`.
    pub name: String,
    /// Driver for this mount.
    pub driver: Arc<dyn FsDriver>,
    /// Auto-assigned mount ID.
    pub id: MountId,
    /// Read-only mount?
    pub read_only: bool,
}

/// Standardized operation results so callers don't have to
/// dig into FsError variants.
pub type VfsResult<T> = Result<T, MountError>;

/// Mount-level error variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountError {
    /// No mount with that name.
    NoSuchMount,
    /// A mount with that name is already registered.
    AlreadyMounted,
    /// Driver returned `DriverError::Unsupported`.
    Unsupported,
    /// Wrapped driver error.
    Driver(crate::driver::DriverError),
}

impl From<crate::driver::DriverError> for MountError {
    fn from(e: crate::driver::DriverError) -> Self {
        MountError::Driver(e)
    }
}

/// Mount table — keyed by mount name.
pub struct MountTable {
    mounts: BTreeMap<String, Arc<MountPoint>>,
    next_id: u32,
}

impl MountTable {
    /// Create an empty mount table.
    pub const fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
            next_id: 0,
        }
    }

    /// Register a new mount. Returns the [`MountId`] assigned by
    /// the table.
    pub fn mount(
        &mut self,
        name: &str,
        driver: Arc<dyn FsDriver>,
        read_only: bool,
    ) -> Result<MountId, MountError> {
        if self.mounts.contains_key(name) {
            return Err(MountError::AlreadyMounted);
        }
        let id = MountId(self.next_id);
        self.next_id += 1;
        let mp = MountPoint {
            name: alloc::string::String::from(name),
            driver,
            id,
            read_only,
        };
        self.mounts.insert(alloc::string::String::from(name), Arc::new(mp));
        Ok(id)
    }

    /// Unregister a mount by name.
    pub fn unmount(&mut self, name: &str) -> Result<(), MountError> {
        self.mounts
            .remove(name)
            .map(|_| ())
            .ok_or(MountError::NoSuchMount)
    }

    /// Resolve an absolute path's mount.
    pub fn resolve<'p>(&self, path: &'p Path) -> Option<(Arc<MountPoint>, &'p str)> {
        let (mount, sub) = path.split_mount()?;
        let mp = self.mounts.get(mount)?;
        Some((mp.clone(), sub))
    }

    /// List all mount names currently in the table.
    pub fn mounts(&self) -> impl Iterator<Item = (&String, &Arc<MountPoint>)> {
        self.mounts.iter()
    }
}

impl Default for MountTable {
    fn default() -> Self {
        Self::new()
    }
}