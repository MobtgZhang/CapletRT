//! Socket descriptor backed by a capability handle.

use alloc::collections::BTreeMap;

use crate::addr::Port;
use crate::interface::NetworkInterface;
use caplet_sync::SpinLock;

/// Socket kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketKind {
    /// UDP datagram socket.
    Udp,
    /// TCP stream socket.
    Tcp,
    /// Raw L3 (ICMP / experiments).
    Raw,
}

/// Errors raised by socket operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketError {
    /// Socket table full.
    OutOfSockets,
    /// Unknown socket id.
    InvalidSocket,
    /// Address already in use.
    AddrInUse,
    /// No listener for that port.
    NoListener,
    /// Buffer too small / would block.
    WouldBlock,
    /// Permission denied by capability check.
    PermissionDenied,
    /// Network is down.
    NotReady,
}

/// Owned socket state.
pub struct Socket {
    /// Kind.
    pub kind: SocketKind,
    /// Local port (0 for ephemeral).
    pub local_port: Port,
    /// Remote peer for connected sockets.
    pub remote: Option<core::net::SocketAddrV4>,
    /// Receive buffer (FIFO).
    pub rx: BTreeMap<u64, alloc::vec::Vec<u8>>,
    /// Next RX sequence number.
    pub next_rx_seq: u64,
    /// Close requested.
    pub closing: bool,
}

/// Socket table. Holds all sockets; the kernel references them
/// by `SocketId`.
pub struct SocketTable<I: NetworkInterface + 'static> {
    /// Sockets, keyed by `SocketId.0`.
    pub sockets: SpinLock<BTreeMap<u32, Socket>>,
    next_id: core::sync::atomic::AtomicU32,
    /// Maximum number of sockets.
    pub capacity: u32,
    /// Reference to the underlying network interface. Held by
    /// `&'static` so the socket table outlives all sockets.
    pub iface: &'static I,
}

/// Unique socket identifier returned by `SocketTable::open`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SocketId(pub u32);

impl<I: NetworkInterface + 'static> SocketTable<I> {
    /// Construct a new socket table.
    pub fn new(capacity: u32, iface: &'static I) -> Self {
        Self {
            sockets: SpinLock::new(BTreeMap::new()),
            next_id: core::sync::atomic::AtomicU32::new(1),
            capacity,
            iface,
        }
    }

    /// Open a new socket and return its id.
    pub fn open(&self, kind: SocketKind) -> Result<SocketId, SocketError> {
        let mut g = self.sockets.lock();
        if g.len() as u32 >= self.capacity {
            return Err(SocketError::OutOfSockets);
        }
        let id = self
            .next_id
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        g.insert(
            id,
            Socket {
                kind,
                local_port: Port::new(0),
                remote: None,
                rx: BTreeMap::new(),
                next_rx_seq: 0,
                closing: false,
            },
        );
        Ok(SocketId(id))
    }

    /// Bind a socket to a local port.
    pub fn bind(&self, sid: SocketId, port: Port) -> Result<(), SocketError> {
        let mut g = self.sockets.lock();
        let collision = g.values().any(|other| other.local_port == port);
        if collision {
            return Err(SocketError::AddrInUse);
        }
        let s = g.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
        if s.local_port.0 != 0 {
            return Err(SocketError::AddrInUse);
        }
        s.local_port = port;
        Ok(())
    }

    /// Receive one packet from a socket's RX queue. Returns
    /// `Err(WouldBlock)` if nothing is buffered.
    pub fn recv(&self, sid: SocketId, buf: &mut [u8]) -> Result<usize, SocketError> {
        let mut g = self.sockets.lock();
        let s = g.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
        let key = s.next_rx_seq;
        let pkt = s.rx.remove(&key).ok_or(SocketError::WouldBlock)?;
        s.next_rx_seq = s.next_rx_seq.wrapping_add(1);
        let n = pkt.len().min(buf.len());
        buf[..n].copy_from_slice(&pkt[..n]);
        Ok(n)
    }

    /// Enqueue a packet onto a socket's RX queue. The caller
    /// is responsible for verifying it matches the destination
    /// socket (port etc.).
    pub fn push_rx(&self, sid: SocketId, pkt: alloc::vec::Vec<u8>) -> Result<(), SocketError> {
        let mut g = self.sockets.lock();
        let s = g.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
        let key = s.next_rx_seq + s.rx.len() as u64;
        s.rx.insert(key, pkt);
        Ok(())
    }

    /// Close a socket. The id is freed.
    pub fn close(&self, sid: SocketId) -> Result<(), SocketError> {
        let mut g = self.sockets.lock();
        g.remove(&sid.0).ok_or(SocketError::InvalidSocket)?;
        Ok(())
    }

    /// Currently open socket count.
    pub fn len(&self) -> usize {
        self.sockets.lock().len()
    }

    /// `true` if no sockets are open.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::LoopbackInterface;

    fn mk_table() -> (SocketTable<LoopbackInterface>, &'static LoopbackInterface) {
        let lb: &'static LoopbackInterface = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            LoopbackInterface::new(),
        ));
        let t = SocketTable::new(4, lb);
        (t, lb)
    }

    #[test]
    fn open_close_sockets() {
        let (t, _) = mk_table();
        let id = t.open(SocketKind::Udp).unwrap();
        assert_eq!(t.len(), 1);
        t.close(id).unwrap();
        assert!(t.is_empty());
    }

    #[test]
    fn bind_collision() {
        let (t, _) = mk_table();
        let a = t.open(SocketKind::Udp).unwrap();
        let b = t.open(SocketKind::Udp).unwrap();
        t.bind(a, Port::new(4242)).unwrap();
        assert_eq!(t.bind(b, Port::new(4242)), Err(SocketError::AddrInUse));
    }

    #[test]
    fn capacity_enforced() {
        let (t, _) = mk_table();
        for _ in 0..4 {
            t.open(SocketKind::Udp).unwrap();
        }
        assert_eq!(t.open(SocketKind::Udp), Err(SocketError::OutOfSockets));
    }

    #[test]
    fn recv_would_block_when_empty() {
        let (t, _) = mk_table();
        let id = t.open(SocketKind::Udp).unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(t.recv(id, &mut buf), Err(SocketError::WouldBlock));
    }

    #[test]
    fn push_then_recv() {
        let (t, _) = mk_table();
        let id = t.open(SocketKind::Udp).unwrap();
        t.push_rx(id, alloc::vec![1, 2, 3, 4]).unwrap();
        let mut buf = [0u8; 4];
        let n = t.recv(id, &mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(buf, [1, 2, 3, 4]);
    }
}