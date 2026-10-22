//! Capability-mediated socket table.
//!
//! Sockets are identified by small integer handles (`SocketId`).  Every
//! operation validates the rights attached to the handle before touching
//! socket state.  The table itself owns the receive queues and the TCP
//! state machines; [`crate::proto`] and [`crate::syscall`] only translate
//! network packets into calls on this API.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use caplet_sync::SpinLock;

use crate::addr::{Ipv4Addr, MacAddr, Port};
use crate::arp::ArpCache;
use crate::interface::{NetworkInterface, TxPacket};
use crate::proto::{self, IpPacket, IpProto};
use crate::tcp::{self, TcpAction, TcpConnection, TcpReply, TcpSegment, TcpState};

/// Maximum number of datagrams buffered per socket.
pub const MAX_RX_QUEUE: usize = 64;

/// First port used for automatic ephemeral allocation (IANA dynamic range).
pub const EPHEMERAL_FIRST: u16 = 49152;
/// Last port used for automatic ephemeral allocation.
pub const EPHEMERAL_LAST: u16 = 65535;

/// Socket kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketKind {
    /// UDP datagram socket.
    Udp,
    /// TCP stream socket.
    Tcp,
    /// Raw IPv4 socket.
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
    /// Network is down or the interface rejected the frame.
    NotReady,
    /// The operation requires a connected socket.
    NotConnected,
    /// The socket is in the wrong state for this operation.
    InvalidState,
}

/// Rights attached to a socket handle.
///
/// Rights form a monotonically decreasing set: a handle can be attenuated
/// but can never regain a right that was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketRights(u8);

impl SocketRights {
    /// Receive data.
    pub const READ: Self = Self(1 << 0);
    /// Send data.
    pub const WRITE: Self = Self(1 << 1);
    /// Initiate an outgoing connection.
    pub const CONNECT: Self = Self(1 << 2);
    /// Accept incoming TCP connections, or enter listen state.
    pub const ACCEPT: Self = Self(1 << 3);
    /// All defined rights.
    pub const ALL: Self = Self(Self::READ.0 | Self::WRITE.0 | Self::CONNECT.0 | Self::ACCEPT.0);

    /// The empty rights set.
    pub const NONE: Self = Self(0);

    /// Whether every right in `other` is present in `self`.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Remove rights. Returns `None` when `requested` contains a right that
    /// this set does not hold (attempting an amplification).
    pub const fn attenuate(self, requested: Self) -> Option<Self> {
        if requested.0 & !self.0 != 0 {
            None
        } else {
            Some(requested)
        }
    }

    /// Raw bit representation.
    pub const fn bits(self) -> u8 {
        self.0
    }
}

impl core::ops::BitOr for SocketRights {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

/// One received datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datagram {
    /// Source IPv4 address.
    pub src: Ipv4Addr,
    /// Source transport port (zero for raw/non-transport packets).
    pub src_port: Port,
    /// Datagram payload.
    pub payload: Vec<u8>,
}

/// Owned socket state.
pub struct Socket {
    /// Kind.
    pub kind: SocketKind,
    /// Local port (0 until an ephemeral port is allocated).
    pub local_port: Port,
    /// Remote peer for connected sockets.
    pub remote: Option<(Ipv4Addr, Port)>,
    /// Receive buffer (FIFO).
    pub rx: VecDeque<Datagram>,
    /// TCP state machine for stream sockets.
    pub tcp: Option<TcpConnection>,
    /// Rights currently attached to this handle.
    pub rights: SocketRights,
    /// Close requested / connection shutdown in progress.
    pub closing: bool,
}

/// Socket table. Holds all sockets; the kernel references them by
/// `SocketId`.
pub struct SocketTable<I: NetworkInterface + 'static> {
    /// Sockets, keyed by `SocketId.0`.
    pub sockets: SpinLock<BTreeMap<u32, Socket>>,
    next_id: AtomicU32,
    /// Maximum number of sockets.
    pub capacity: u32,
    /// Reference to the underlying network interface.
    pub iface: &'static I,
    /// ARP/neighbour cache used when transmitting.
    pub neighbors: ArpCache,
}

/// Unique socket identifier returned by [`SocketTable::open`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SocketId(pub u32);

/// Return whether a table currently holding `current_len` sockets can
/// admit one more socket when its configured capacity is `capacity`.
///
/// This is deliberately a small pure predicate: `SocketTable::open` uses
/// it as the single admission check, which lets bounded verification cover
/// the capacity invariant without modelling the socket map's allocator.
#[inline]
pub fn capacity_allows(current_len: usize, capacity: u32) -> bool {
    capacity != 0 && current_len < capacity as usize
}

impl<I: NetworkInterface + 'static> SocketTable<I> {
    /// Construct a new socket table.
    pub fn new(capacity: u32, iface: &'static I) -> Self {
        Self {
            sockets: SpinLock::new(BTreeMap::new()),
            next_id: AtomicU32::new(1),
            capacity,
            iface,
            neighbors: ArpCache::new(),
        }
    }

    /// Open a new socket and return its id.
    pub fn open(&self, kind: SocketKind) -> Result<SocketId, SocketError> {
        let mut sockets = self.sockets.lock();
        if !capacity_allows(sockets.len(), self.capacity) {
            return Err(SocketError::OutOfSockets);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let tcp = match kind {
            SocketKind::Tcp => Some(TcpConnection::new(id.wrapping_mul(0x9e37_79b9) | 1)),
            _ => None,
        };
        sockets.insert(
            id,
            Socket {
                kind,
                local_port: Port::new(0),
                remote: None,
                rx: VecDeque::new(),
                tcp,
                rights: SocketRights::ALL,
                closing: false,
            },
        );
        Ok(SocketId(id))
    }

    /// Bind a socket to a local port. A wildcard port (`0`) requests an
    /// ephemeral port from the dynamic range. The selected port is returned.
    pub fn bind(&self, sid: SocketId, port: Port) -> Result<Port, SocketError> {
        let mut sockets = self.sockets.lock();
        let kind = {
            let socket = sockets.get(&sid.0).ok_or(SocketError::InvalidSocket)?;
            Self::require(socket, SocketRights::WRITE)?;
            socket.kind
        };
        if port.0 != 0
            && sockets
                .values()
                .any(|other| other.kind == kind && other.local_port == port)
        {
            return Err(SocketError::AddrInUse);
        }
        let selected = if port.0 != 0 {
            port
        } else {
            (EPHEMERAL_FIRST..=EPHEMERAL_LAST)
                .find(|candidate| {
                    !sockets
                        .values()
                        .any(|other| other.kind == kind && other.local_port.0 == *candidate)
                })
                .map(Port)
                .ok_or(SocketError::AddrInUse)?
        };
        let socket = sockets.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
        if socket.local_port.0 != 0 {
            return Err(SocketError::InvalidState);
        }
        socket.local_port = selected;
        Ok(selected)
    }

    /// Remove rights from a socket handle. Rights can only be removed.
    pub fn attenuate(&self, sid: SocketId, rights: SocketRights) -> Result<(), SocketError> {
        let mut sockets = self.sockets.lock();
        let socket = sockets.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
        let narrowed = socket
            .rights
            .attenuate(rights)
            .ok_or(SocketError::PermissionDenied)?;
        socket.rights = narrowed;
        Ok(())
    }

    /// Read the current rights without changing them.
    pub fn rights(&self, sid: SocketId) -> Result<SocketRights, SocketError> {
        let sockets = self.sockets.lock();
        Ok(sockets
            .get(&sid.0)
            .ok_or(SocketError::InvalidSocket)?
            .rights)
    }

    /// Kind of an open socket.
    pub fn kind(&self, sid: SocketId) -> Result<SocketKind, SocketError> {
        let sockets = self.sockets.lock();
        Ok(sockets.get(&sid.0).ok_or(SocketError::InvalidSocket)?.kind)
    }

    /// Remote peer of a connected socket.
    pub fn remote(&self, sid: SocketId) -> Result<Option<(Ipv4Addr, Port)>, SocketError> {
        let sockets = self.sockets.lock();
        Ok(sockets
            .get(&sid.0)
            .ok_or(SocketError::InvalidSocket)?
            .remote)
    }

    /// Install an ARP entry used by subsequent transmissions.
    pub fn set_neighbor(&self, ip: Ipv4Addr, mac: MacAddr) {
        self.neighbors.insert(ip, mac);
    }

    /// Connect a UDP socket to a peer, or initiate a TCP handshake.
    pub fn connect(
        &self,
        sid: SocketId,
        remote_ip: Ipv4Addr,
        remote_port: Port,
    ) -> Result<(), SocketError> {
        let mut sockets = self.sockets.lock();
        let (kind, local_port) = {
            let socket = sockets.get(&sid.0).ok_or(SocketError::InvalidSocket)?;
            Self::require(socket, SocketRights::CONNECT)?;
            (socket.kind, socket.local_port)
        };
        if remote_port.is_unspecified() {
            return Err(SocketError::InvalidState);
        }
        let local_port = if local_port.is_unspecified() {
            allocate_ephemeral(&sockets, kind)?
        } else {
            local_port
        };
        let socket = sockets.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
        socket.local_port = local_port;
        socket.remote = Some((remote_ip, remote_port));
        let syn = if socket.kind == SocketKind::Tcp {
            let tcp = socket.tcp.as_mut().ok_or(SocketError::InvalidState)?;
            Some(tcp.connect())
        } else {
            None
        };
        let local_ip = self.iface.ipv4();
        let local_port = socket.local_port;
        drop(sockets);
        if let Some(reply) = syn {
            self.transmit_tcp_reply(local_ip, remote_ip, local_port, remote_port, &reply)?;
        }
        Ok(())
    }

    /// Enter passive-open state. The caller should bind first; an unbound
    /// listener receives an ephemeral port.
    pub fn listen(&self, sid: SocketId) -> Result<Port, SocketError> {
        let selected = {
            let mut sockets = self.sockets.lock();
            let (kind, local_port) = {
                let socket = sockets.get(&sid.0).ok_or(SocketError::InvalidSocket)?;
                Self::require(socket, SocketRights::ACCEPT)?;
                (socket.kind, socket.local_port)
            };
            if kind != SocketKind::Tcp {
                return Err(SocketError::InvalidState);
            }
            let selected = if local_port.is_unspecified() {
                allocate_ephemeral(&sockets, kind)?
            } else {
                local_port
            };
            let socket = sockets.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
            socket.local_port = selected;
            socket
                .tcp
                .as_mut()
                .ok_or(SocketError::InvalidState)?
                .listen();
            socket.local_port
        };
        Ok(selected)
    }

    /// Send a UDP datagram. Returns the number of application bytes sent.
    pub fn send_udp(
        &self,
        sid: SocketId,
        dst_ip: Ipv4Addr,
        dst_port: Port,
        payload: &[u8],
    ) -> Result<usize, SocketError> {
        let (local_ip, local_port) = {
            let mut sockets = self.sockets.lock();
            let (kind, local_port, remote) = {
                let socket = sockets.get(&sid.0).ok_or(SocketError::InvalidSocket)?;
                Self::require(socket, SocketRights::WRITE)?;
                (socket.kind, socket.local_port, socket.remote)
            };
            if kind != SocketKind::Udp {
                return Err(SocketError::InvalidState);
            }
            let local_port = if local_port.is_unspecified() {
                allocate_ephemeral(&sockets, kind)?
            } else {
                local_port
            };
            if let Some((ip, port)) = remote {
                if ip != dst_ip || port != dst_port {
                    return Err(SocketError::InvalidState);
                }
            }
            let socket = sockets.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
            socket.local_port = local_port;
            (self.iface.ipv4(), local_port)
        };
        let udp = proto::build_udp(local_ip, dst_ip, local_port, dst_port, payload);
        let ip = proto::build_ipv4(local_ip, dst_ip, IpProto::Udp, &udp, 0, 64);
        self.transmit_l3(dst_ip, ip)?;
        Ok(payload.len())
    }

    /// Send on a connected TCP stream.
    pub fn send_tcp(&self, sid: SocketId, payload: &[u8]) -> Result<usize, SocketError> {
        let (local_ip, local_port, remote_ip, remote_port, reply) = {
            let mut sockets = self.sockets.lock();
            let socket = sockets.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
            Self::require(socket, SocketRights::WRITE)?;
            if socket.kind != SocketKind::Tcp {
                return Err(SocketError::InvalidState);
            }
            let (remote_ip, remote_port) = socket.remote.ok_or(SocketError::NotConnected)?;
            let reply = socket
                .tcp
                .as_mut()
                .ok_or(SocketError::InvalidState)?
                .send(payload)
                .ok_or(SocketError::NotConnected)?;
            (
                self.iface.ipv4(),
                socket.local_port,
                remote_ip,
                remote_port,
                reply,
            )
        };
        self.transmit_tcp_reply(local_ip, remote_ip, local_port, remote_port, &reply)?;
        Ok(payload.len())
    }

    /// Send to an explicit destination. UDP forwards to [`Self::send_udp`];
    /// raw sockets accept complete IPv4 packets here. TCP must already be
    /// connected and uses [`Self::send_tcp`].
    pub fn send_to(
        &self,
        sid: SocketId,
        dst_ip: Ipv4Addr,
        dst_port: Port,
        payload: &[u8],
    ) -> Result<usize, SocketError> {
        match self.kind(sid)? {
            SocketKind::Udp => self.send_udp(sid, dst_ip, dst_port, payload),
            SocketKind::Raw => self.send_raw(sid, dst_ip, payload),
            SocketKind::Tcp => Err(SocketError::InvalidState),
        }
    }

    /// Send on a connected UDP/TCP socket. Raw sockets require an explicit
    /// destination and therefore return `NotConnected` here.
    pub fn send(&self, sid: SocketId, payload: &[u8]) -> Result<usize, SocketError> {
        match self.kind(sid)? {
            SocketKind::Udp => {
                let (ip, port) = self.remote(sid)?.ok_or(SocketError::NotConnected)?;
                self.send_udp(sid, ip, port, payload)
            }
            SocketKind::Tcp => self.send_tcp(sid, payload),
            SocketKind::Raw => Err(SocketError::NotConnected),
        }
    }

    /// Send a raw IPv4 packet (the buffer must already contain the IP
    /// header). Raw sockets are intended for diagnostics and ICMP tools.
    pub fn send_raw(
        &self,
        sid: SocketId,
        dst_ip: Ipv4Addr,
        packet: &[u8],
    ) -> Result<usize, SocketError> {
        {
            let sockets = self.sockets.lock();
            let socket = sockets.get(&sid.0).ok_or(SocketError::InvalidSocket)?;
            Self::require(socket, SocketRights::WRITE)?;
            if socket.kind != SocketKind::Raw {
                return Err(SocketError::InvalidState);
            }
        }
        self.transmit_l3(dst_ip, packet.to_vec())?;
        Ok(packet.len())
    }

    /// Close a socket. For an established TCP connection a best-effort FIN
    /// is emitted before the handle is released.
    pub fn close(&self, sid: SocketId) -> Result<(), SocketError> {
        let (socket, local_ip) = {
            let mut sockets = self.sockets.lock();
            let mut socket = sockets.remove(&sid.0).ok_or(SocketError::InvalidSocket)?;
            socket.closing = true;
            (socket, self.iface.ipv4())
        };
        if socket.kind == SocketKind::Tcp {
            if let (Some((remote_ip, remote_port)), Some(tcp)) =
                (socket.remote, socket.tcp.as_ref())
            {
                if matches!(tcp.state, TcpState::Established | TcpState::CloseWait) {
                    let mut tcp = tcp.clone();
                    if let Some(reply) = tcp.close() {
                        let _ = self.transmit_tcp_reply(
                            local_ip,
                            remote_ip,
                            socket.local_port,
                            remote_port,
                            &reply,
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Receive one datagram, discarding the source address.
    pub fn recv(&self, sid: SocketId, buf: &mut [u8]) -> Result<usize, SocketError> {
        self.recv_from(sid, buf).map(|(n, _, _)| n)
    }

    /// Receive one datagram and report its source.
    pub fn recv_from(
        &self,
        sid: SocketId,
        buf: &mut [u8],
    ) -> Result<(usize, Ipv4Addr, Port), SocketError> {
        let mut sockets = self.sockets.lock();
        let socket = sockets.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
        Self::require(socket, SocketRights::READ)?;
        let datagram = socket.rx.pop_front().ok_or(SocketError::WouldBlock)?;
        let n = datagram.payload.len().min(buf.len());
        buf[..n].copy_from_slice(&datagram.payload[..n]);
        Ok((n, datagram.src, datagram.src_port))
    }

    /// Enqueue a datagram without a meaningful source.
    pub fn push_rx(&self, sid: SocketId, payload: Vec<u8>) -> Result<(), SocketError> {
        self.push_rx_from(sid, Ipv4Addr::UNSPECIFIED, Port::new(0), payload)
    }

    /// Enqueue a datagram with its source address. Oldest datagrams are
    /// dropped when the per-socket FIFO is full.
    pub fn push_rx_from(
        &self,
        sid: SocketId,
        src: Ipv4Addr,
        src_port: Port,
        payload: Vec<u8>,
    ) -> Result<(), SocketError> {
        let mut sockets = self.sockets.lock();
        let socket = sockets.get_mut(&sid.0).ok_or(SocketError::InvalidSocket)?;
        if socket.rx.len() >= MAX_RX_QUEUE {
            socket.rx.pop_front();
        }
        socket.rx.push_back(Datagram {
            src,
            src_port,
            payload,
        });
        Ok(())
    }

    /// Find the UDP socket that should receive a datagram.
    pub fn find_udp(&self, dst_port: Port, remote: Option<(Ipv4Addr, Port)>) -> Option<SocketId> {
        let sockets = self.sockets.lock();
        sockets.iter().find_map(|(id, socket)| {
            if socket.kind != SocketKind::Udp || socket.local_port != dst_port {
                return None;
            }
            if let (Some(expected), Some(actual)) = (socket.remote, remote) {
                if expected != actual {
                    return None;
                }
            }
            Some(SocketId(*id))
        })
    }

    /// Feed a parsed IPv4/TCP packet into the matching connection. Returns
    /// `NoListener` only when a RST could not be transmitted; an unknown
    /// segment normally produces a RST and is otherwise handled.
    pub fn handle_tcp(&self, pkt: &IpPacket<'_>, reply_mac: MacAddr) -> Result<(), SocketError> {
        let Some(seg) = tcp::parse_tcp_checked(pkt.payload, pkt.src, pkt.dst) else {
            return Ok(());
        };

        let sid = {
            let sockets = self.sockets.lock();
            sockets.iter().find_map(|(id, socket)| {
                if socket.kind != SocketKind::Tcp || socket.local_port != seg.dst {
                    return None;
                }
                match socket.remote {
                    Some((ip, port)) if ip == pkt.src && port == seg.src => Some(*id),
                    None if matches!(
                        socket.tcp.as_ref().map(|tcp| tcp.state),
                        Some(TcpState::Listen)
                    ) =>
                    {
                        Some(*id)
                    }
                    _ => None,
                }
            })
        };
        let Some(sid) = sid else {
            return self.send_tcp_reset(pkt, &seg, reply_mac);
        };

        let (local_port, action, remote_ip, remote_port) = {
            let mut sockets = self.sockets.lock();
            let socket = sockets.get_mut(&sid).ok_or(SocketError::InvalidSocket)?;
            if socket.remote.is_none() {
                socket.remote = Some((pkt.src, seg.src));
            }
            let local_port = socket.local_port;
            let action = socket
                .tcp
                .as_mut()
                .ok_or(SocketError::InvalidState)?
                .on_segment(&seg);
            (local_port, action, pkt.src, seg.src)
        };
        match action {
            TcpAction::None | TcpAction::Established => Ok(()),
            TcpAction::Send(reply) => {
                self.transmit_tcp_reply(pkt.dst, remote_ip, local_port, remote_port, &reply)?;
                Ok(())
            }
            TcpAction::Deliver(payload, ack) => {
                self.push_rx_from(SocketId(sid), remote_ip, remote_port, payload)?;
                self.transmit_tcp_reply(pkt.dst, remote_ip, local_port, remote_port, &ack)?;
                Ok(())
            }
            TcpAction::Closed => Ok(()),
        }
    }

    /// Currently open socket count.
    pub fn len(&self) -> usize {
        self.sockets.lock().len()
    }

    /// `true` if no sockets are open.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn require(socket: &Socket, right: SocketRights) -> Result<(), SocketError> {
        if socket.rights.contains(right) {
            Ok(())
        } else {
            Err(SocketError::PermissionDenied)
        }
    }

    fn transmit_l3(&self, dst_ip: Ipv4Addr, payload: Vec<u8>) -> Result<(), SocketError> {
        let dst = self.resolve_mac(dst_ip);
        self.iface
            .transmit(TxPacket {
                dst,
                payload,
                ethertype: None,
            })
            .map_err(|_| SocketError::NotReady)
    }

    fn resolve_mac(&self, dst_ip: Ipv4Addr) -> MacAddr {
        if let Some(mac) = self.neighbors.lookup(dst_ip) {
            return mac;
        }
        if dst_ip.is_loopback() || dst_ip == self.iface.ipv4() {
            self.iface.mac()
        } else {
            MacAddr::BROADCAST
        }
    }

    fn transmit_tcp_reply(
        &self,
        local_ip: Ipv4Addr,
        remote_ip: Ipv4Addr,
        local_port: Port,
        remote_port: Port,
        reply: &TcpReply,
    ) -> Result<(), SocketError> {
        let segment = tcp::build_tcp(
            local_ip,
            remote_ip,
            local_port,
            remote_port,
            reply.seq,
            reply.ack,
            reply.flags,
            TcpConnection::new(0).rcv_wnd,
            &reply.payload,
        );
        let packet = proto::build_ipv4(local_ip, remote_ip, IpProto::Tcp, &segment, 0, 64);
        self.transmit_l3(remote_ip, packet)
    }

    fn send_tcp_reset(
        &self,
        pkt: &IpPacket<'_>,
        seg: &TcpSegment<'_>,
        reply_mac: MacAddr,
    ) -> Result<(), SocketError> {
        let seq = if seg.has(tcp::flags::ACK) { seg.ack } else { 0 };
        let ack = seg.seq.wrapping_add(seg.sequence_len());
        let segment = tcp::build_tcp(
            pkt.dst,
            pkt.src,
            seg.dst,
            seg.src,
            seq,
            ack,
            tcp::flags::RST,
            0,
            &[],
        );
        let packet = proto::build_ipv4(pkt.dst, pkt.src, IpProto::Tcp, &segment, 0, 64);
        self.iface
            .transmit(TxPacket {
                dst: reply_mac,
                payload: packet,
                ethertype: None,
            })
            .map_err(|_| SocketError::NotReady)
    }
}

fn allocate_ephemeral(
    sockets: &BTreeMap<u32, Socket>,
    kind: SocketKind,
) -> Result<Port, SocketError> {
    (EPHEMERAL_FIRST..=EPHEMERAL_LAST)
        .find(|candidate| {
            !sockets
                .values()
                .any(|other| other.kind == kind && other.local_port.0 == *candidate)
        })
        .map(Port)
        .ok_or(SocketError::AddrInUse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icmp;
    use crate::interface::LoopbackInterface;

    fn mk_table() -> (SocketTable<LoopbackInterface>, &'static LoopbackInterface) {
        let iface: &'static LoopbackInterface =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(LoopbackInterface::new()));
        iface.set_ipv4(Ipv4Addr::new([10, 0, 0, 2]));
        (SocketTable::new(8, iface), iface)
    }

    #[test]
    fn open_close_sockets() {
        let (table, _) = mk_table();
        let id = table.open(SocketKind::Udp).unwrap();
        assert_eq!(table.len(), 1);
        table.close(id).unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn capacity_enforced() {
        let (table, _) = mk_table();
        for _ in 0..8 {
            table.open(SocketKind::Udp).unwrap();
        }
        assert_eq!(table.open(SocketKind::Udp), Err(SocketError::OutOfSockets));
    }

    #[test]
    fn bind_is_per_protocol_and_allocates_ephemeral_ports() {
        let (table, _) = mk_table();
        let udp = table.open(SocketKind::Udp).unwrap();
        let tcp = table.open(SocketKind::Tcp).unwrap();
        assert_eq!(table.bind(udp, Port::new(4242)), Ok(Port::new(4242)));
        assert_eq!(table.bind(tcp, Port::new(4242)), Ok(Port::new(4242)));
        let other = table.open(SocketKind::Udp).unwrap();
        assert_eq!(
            table.bind(other, Port::new(4242)),
            Err(SocketError::AddrInUse)
        );
        let ephemeral = table.bind(other, Port::new(0)).unwrap();
        assert!((EPHEMERAL_FIRST..=EPHEMERAL_LAST).contains(&ephemeral.0));
    }

    #[test]
    fn rights_can_only_be_attenuated() {
        let (table, _) = mk_table();
        let id = table.open(SocketKind::Udp).unwrap();
        table.attenuate(id, SocketRights::READ).unwrap();
        assert_eq!(table.rights(id), Ok(SocketRights::READ));
        assert_eq!(
            table.attenuate(id, SocketRights::READ | SocketRights::WRITE),
            Err(SocketError::PermissionDenied)
        );
    }

    #[test]
    fn rights_are_enforced_on_send() {
        let (table, iface) = mk_table();
        let id = table.open(SocketKind::Udp).unwrap();
        table.attenuate(id, SocketRights::READ).unwrap();
        assert_eq!(
            table.send_udp(id, Ipv4Addr::new([10, 0, 0, 1]), Port::new(80), b"x"),
            Err(SocketError::PermissionDenied)
        );
        assert!(iface.drain_tx().is_empty());
    }

    #[test]
    fn recv_would_block_when_empty() {
        let (table, _) = mk_table();
        let id = table.open(SocketKind::Udp).unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(table.recv(id, &mut buf), Err(SocketError::WouldBlock));
    }

    #[test]
    fn rx_queue_is_bounded_and_fifo() {
        let (table, _) = mk_table();
        let id = table.open(SocketKind::Udp).unwrap();
        for value in 0..(MAX_RX_QUEUE + 2) {
            table.push_rx(id, alloc::vec![value as u8]).unwrap();
        }
        let mut buf = [0u8; 1];
        assert_eq!(table.recv(id, &mut buf), Ok(1));
        assert_eq!(buf, [2]);
    }

    #[test]
    fn udp_send_builds_valid_wire_packet() {
        let (table, iface) = mk_table();
        let id = table.open(SocketKind::Udp).unwrap();
        table.bind(id, Port::new(5353)).unwrap();
        assert_eq!(
            table.send_udp(id, Ipv4Addr::new([10, 0, 0, 1]), Port::new(1234), b"hello"),
            Ok(5)
        );
        let tx = iface.drain_tx();
        assert_eq!(tx.len(), 1);
        let pkt = proto::parse_ipv4(&tx[0].payload).unwrap();
        let udp = proto::parse_udp_checked(pkt.payload, pkt.src, pkt.dst).unwrap();
        assert_eq!(udp.src_port, Port::new(5353));
        assert_eq!(udp.dst_port, Port::new(1234));
        assert_eq!(udp.payload, b"hello");
    }

    #[test]
    fn udp_dispatch_matches_port_and_remote() {
        let (table, iface) = mk_table();
        let id = table.open(SocketKind::Udp).unwrap();
        table.bind(id, Port::new(5000)).unwrap();
        let src = Ipv4Addr::new([10, 0, 0, 1]);
        let local = iface.ipv4();
        let udp = proto::build_udp(src, local, Port::new(99), Port::new(5000), b"abc");
        let raw = proto::build_ipv4(src, local, IpProto::Udp, &udp, 1, 64);
        let pkt = proto::parse_ipv4(&raw).unwrap();
        proto::dispatch_inbound(&table, &pkt, iface.mac()).unwrap();
        let mut buf = [0u8; 8];
        let (n, from, port) = table.recv_from(id, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"abc");
        assert_eq!(from, src);
        assert_eq!(port, Port::new(99));
    }

    #[test]
    fn tcp_handshake_and_data_flow_through_table() {
        let (table, iface) = mk_table();
        let listener = table.open(SocketKind::Tcp).unwrap();
        table.bind(listener, Port::new(80)).unwrap();
        table.listen(listener).unwrap();

        let remote = Ipv4Addr::new([10, 0, 0, 1]);
        let local = iface.ipv4();
        let syn = tcp::build_tcp(
            remote,
            local,
            Port::new(40000),
            Port::new(80),
            1000,
            0,
            tcp::flags::SYN,
            4096,
            &[],
        );
        let raw = proto::build_ipv4(remote, local, IpProto::Tcp, &syn, 1, 64);
        let pkt = proto::parse_ipv4(&raw).unwrap();
        table.handle_tcp(&pkt, iface.mac()).unwrap();
        let synack = iface.drain_tx();
        assert_eq!(synack.len(), 1);
        let reply = proto::parse_ipv4(&synack[0].payload).unwrap();
        let seg = tcp::parse_tcp_checked(reply.payload, reply.src, reply.dst).unwrap();
        assert_eq!(seg.flags, tcp::flags::SYN | tcp::flags::ACK);

        let ack = tcp::build_tcp(
            remote,
            local,
            Port::new(40000),
            Port::new(80),
            seg.ack,
            seg.seq.wrapping_add(1),
            tcp::flags::ACK,
            4096,
            &[],
        );
        let raw = proto::build_ipv4(remote, local, IpProto::Tcp, &ack, 2, 64);
        let pkt = proto::parse_ipv4(&raw).unwrap();
        table.handle_tcp(&pkt, iface.mac()).unwrap();

        let data = tcp::build_tcp(
            remote,
            local,
            Port::new(40000),
            Port::new(80),
            seg.ack,
            seg.seq.wrapping_add(1),
            tcp::flags::ACK | tcp::flags::PSH,
            4096,
            b"ping",
        );
        let raw = proto::build_ipv4(remote, local, IpProto::Tcp, &data, 3, 64);
        let pkt = proto::parse_ipv4(&raw).unwrap();
        table.handle_tcp(&pkt, iface.mac()).unwrap();
        let mut buf = [0u8; 8];
        let (n, from, port) = table.recv_from(listener, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"ping");
        assert_eq!(from, remote);
        assert_eq!(port, Port::new(40000));
    }

    #[test]
    fn unknown_tcp_port_gets_reset() {
        let (table, iface) = mk_table();
        let remote = Ipv4Addr::new([10, 0, 0, 1]);
        let local = iface.ipv4();
        let syn = tcp::build_tcp(
            remote,
            local,
            Port::new(40000),
            Port::new(9999),
            1,
            0,
            tcp::flags::SYN,
            4096,
            &[],
        );
        let raw = proto::build_ipv4(remote, local, IpProto::Tcp, &syn, 1, 64);
        let pkt = proto::parse_ipv4(&raw).unwrap();
        table.handle_tcp(&pkt, iface.mac()).unwrap();
        let tx = iface.drain_tx();
        assert_eq!(tx.len(), 1);
        let reply = proto::parse_ipv4(&tx[0].payload).unwrap();
        let seg = tcp::parse_tcp_checked(reply.payload, reply.src, reply.dst).unwrap();
        assert!(seg.has(tcp::flags::RST));
    }

    #[test]
    fn handcrafted_icmp_checksum_is_valid() {
        let request = icmp::build_echo_request(1, 1, b"x");
        assert_eq!(icmp::checksum(&request), 0);
    }
}
