//! Capability-mediated socket handle.
//!
//! A [`SocketCap`] is the user-facing handle for a socket. It
//! is constructed from a [`SocketId`] (which is the raw handle
//! the kernel returns) and on [`Drop`] it returns the socket
//! to the table, ensuring resources are released even if the
//! user forgets to call `close()`.

use crate::socket::{SocketError, SocketTable};

/// Capability-protected socket handle. Drops the socket on
/// destruction.
pub struct SocketCap<'a, I: crate::interface::NetworkInterface + 'static> {
    id: crate::socket::SocketId,
    table: &'a SocketTable<I>,
}

impl<'a, I: crate::interface::NetworkInterface + 'static> SocketCap<'a, I> {
    /// Wrap an existing socket id. Does not check liveness;
    /// the caller is responsible for ensuring the id is valid.
    pub fn wrap(
        id: crate::socket::SocketId,
        table: &'a SocketTable<I>,
    ) -> Self {
        Self { id, table }
    }

    /// Underlying id.
    pub fn id(&self) -> crate::socket::SocketId {
        self.id
    }

    /// Consume the handle and return the id without closing.
    pub fn into_id(self) -> crate::socket::SocketId {
        self.id
    }

    /// Close the socket now (no Drop).
    pub fn close(self) -> Result<(), SocketError> {
        let SocketCap { id, table } = self;
        table.close(id)
    }
}

impl<'a, I: crate::interface::NetworkInterface + 'static> Drop for SocketCap<'a, I> {
    fn drop(&mut self) {
        // Best-effort close. Errors are ignored because we are
        // already on the cleanup path.
        let _ = self.table.close(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::LoopbackInterface;
    use crate::socket::SocketKind;

    fn mk() -> (SocketTable<LoopbackInterface>, &'static LoopbackInterface) {
        let lb: &'static LoopbackInterface = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            LoopbackInterface::new(),
        ));
        let t = SocketTable::new(4, lb);
        (t, lb)
    }

    #[test]
    fn cap_releases_on_drop() {
        let (t, _) = mk();
        {
            let id = t.open(SocketKind::Udp).unwrap();
            let _cap = SocketCap::wrap(id, &t);
            assert_eq!(t.len(), 1);
        }
        // After the cap is dropped, the table should be empty.
        assert!(t.is_empty());
    }

    #[test]
    fn cap_close_releases_now() {
        let (t, _) = mk();
        let id = t.open(SocketKind::Udp).unwrap();
        let cap = SocketCap::wrap(id, &t);
        cap.close().unwrap();
        assert!(t.is_empty());
    }
}