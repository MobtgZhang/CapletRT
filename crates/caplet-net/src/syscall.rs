//! `syscall_net` — kernel entry point for network operations.
//!
//! The syscall layer is deliberately thin: capability/right checks and all
//! socket state live in [`crate::socket`], while packet parsing lives in
//! [`crate::proto`]. This module translates syscall tags and drains the
//! interface receive queue.

use crate::addr::{Ipv4Addr, MacAddr, Port};
use crate::arp;
use crate::eth;
use crate::interface::{NetworkInterface, RxPacket, TxPacket};
use crate::proto;
use crate::socket::{SocketError, SocketId, SocketKind, SocketTable};

/// Top-level network syscall tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetSyscall {
    /// Open a new socket.
    Socket(SocketKind),
    /// Bind a socket to a local port (zero requests an ephemeral port).
    Bind,
    /// Send on a connected UDP/TCP socket.
    Send,
    /// Send to an explicit destination.
    SendTo {
        /// Destination IPv4 address.
        addr: Ipv4Addr,
        /// Destination port (ignored by raw sockets).
        port: Port,
    },
    /// Receive bytes from a socket.
    Recv,
    /// Receive one datagram and its source address.
    RecvFrom,
    /// Connect a UDP socket or start a TCP handshake.
    Connect {
        /// Peer IPv4 address.
        addr: Ipv4Addr,
        /// Peer port.
        port: Port,
    },
    /// Enter passive-open state on a TCP socket.
    Listen,
    /// Close a socket.
    Close,
    /// Poll the interface for one incoming frame.
    PollRx,
    /// Get the interface MAC.
    Mac,
    /// Get the interface IPv4.
    Ip,
}

/// Result of [`syscall_net`].
#[derive(Debug, PartialEq, Eq)]
pub enum NetResult {
    /// New socket id returned.
    NewSocket(SocketId),
    /// Number of bytes received / sent.
    Count(usize),
    /// Interface MAC.
    Mac(MacAddr),
    /// Interface IPv4 address.
    Ip(Ipv4Addr),
    /// A selected local port (bind/listen).
    Port(Port),
    /// Datagram length and source.
    Datagram {
        /// Number of bytes copied into the caller's buffer.
        len: usize,
        /// Source IPv4 address.
        src: Ipv4Addr,
        /// Source port.
        port: Port,
    },
    /// No incoming packet right now.
    NoPacket,
    /// `Ok(())` — for bind / close.
    Ok,
}

/// Apply one network syscall.
pub fn syscall_net<I: NetworkInterface + 'static>(
    table: &SocketTable<I>,
    op: NetSyscall,
    sid: SocketId,
    port: Port,
    buf: &mut [u8],
) -> Result<NetResult, SocketError> {
    match op {
        NetSyscall::Socket(kind) => Ok(NetResult::NewSocket(table.open(kind)?)),
        NetSyscall::Bind => {
            table.bind(sid, port)?;
            Ok(NetResult::Ok)
        }
        NetSyscall::Send => Ok(NetResult::Count(table.send(sid, buf)?)),
        NetSyscall::SendTo { addr, port } => {
            Ok(NetResult::Count(table.send_to(sid, addr, port, buf)?))
        }
        NetSyscall::Recv => Ok(NetResult::Count(table.recv(sid, buf)?)),
        NetSyscall::RecvFrom => {
            let (len, src, port) = table.recv_from(sid, buf)?;
            Ok(NetResult::Datagram { len, src, port })
        }
        NetSyscall::Connect { addr, port } => {
            table.connect(sid, addr, port)?;
            Ok(NetResult::Ok)
        }
        NetSyscall::Listen => Ok(NetResult::Port(table.listen(sid)?)),
        NetSyscall::Close => {
            table.close(sid)?;
            Ok(NetResult::Ok)
        }
        NetSyscall::PollRx => {
            let rx = table.iface.poll_rx().map_err(|_| SocketError::NotReady)?;
            match rx {
                Some(pkt) => {
                    let count = handle_inbound(table, &pkt)?;
                    Ok(NetResult::Count(count))
                }
                None => Ok(NetResult::NoPacket),
            }
        }
        NetSyscall::Mac => Ok(NetResult::Mac(table.iface.mac())),
        NetSyscall::Ip => Ok(NetResult::Ip(table.iface.ipv4())),
    }
}

/// Process one Ethernet frame. Returns the number of L3 bytes consumed.
fn handle_inbound<I: NetworkInterface + 'static>(
    table: &SocketTable<I>,
    pkt: &RxPacket,
) -> Result<usize, SocketError> {
    match pkt.ethertype {
        eth::ethertype::ARP => {
            let Some(request) = arp::parse_arp(&pkt.payload) else {
                return Ok(0);
            };
            table.neighbors.observe(&request);
            let local_ip = table.iface.ipv4();
            if request.operation == arp::OP_REQUEST
                && !local_ip.is_unspecified()
                && (request.target_ip == local_ip || request.target_ip.is_broadcast())
            {
                let reply = arp::build_arp(
                    arp::OP_REPLY,
                    table.iface.mac(),
                    local_ip,
                    request.sender_mac,
                    request.sender_ip,
                );
                table
                    .iface
                    .transmit(TxPacket {
                        dst: request.sender_mac,
                        payload: reply,
                        ethertype: Some(eth::ethertype::ARP),
                    })
                    .map_err(|_| SocketError::NotReady)?;
            }
            Ok(pkt.payload.len())
        }
        eth::ethertype::IPV4 => {
            let Some(ip) = proto::parse_ipv4(&pkt.payload) else {
                return Ok(0);
            };
            if !ip.src.is_unspecified() {
                table.neighbors.insert(ip.src, pkt.src);
            }
            proto::dispatch_inbound(table, &ip, pkt.src)?;
            Ok(ip.total_len)
        }
        _ => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::LoopbackInterface;

    fn mk() -> (SocketTable<LoopbackInterface>, &'static LoopbackInterface) {
        let iface: &'static LoopbackInterface =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(LoopbackInterface::new()));
        iface.set_ipv4(Ipv4Addr::new([10, 0, 0, 2]));
        (SocketTable::new(4, iface), iface)
    }

    fn open(table: &SocketTable<LoopbackInterface>, kind: SocketKind) -> SocketId {
        match syscall_net(
            table,
            NetSyscall::Socket(kind),
            SocketId(0),
            Port::new(0),
            &mut [],
        )
        .unwrap()
        {
            NetResult::NewSocket(id) => id,
            other => panic!("expected NewSocket, got {other:?}"),
        }
    }

    #[test]
    fn syscall_open_then_close() {
        let (table, _) = mk();
        let id = open(&table, SocketKind::Udp);
        syscall_net(&table, NetSyscall::Close, id, Port::new(0), &mut []).unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn syscall_mac_and_ip() {
        let (table, iface) = mk();
        assert_eq!(
            syscall_net(&table, NetSyscall::Mac, SocketId(0), Port::new(0), &mut []).unwrap(),
            NetResult::Mac(iface.mac())
        );
        assert_eq!(
            syscall_net(&table, NetSyscall::Ip, SocketId(0), Port::new(0), &mut []).unwrap(),
            NetResult::Ip(Ipv4Addr::new([10, 0, 0, 2]))
        );
    }

    #[test]
    fn syscall_poll_rx_dispatches_to_socket() {
        let (table, iface) = mk();
        let id = open(&table, SocketKind::Udp);
        syscall_net(&table, NetSyscall::Bind, id, Port::new(1234), &mut []).unwrap();
        let src = Ipv4Addr::new([10, 0, 0, 1]);
        let udp = proto::build_udp(src, iface.ipv4(), Port::new(5060), Port::new(1234), b"ok");
        let ip = proto::build_ipv4(src, iface.ipv4(), proto::IpProto::Udp, &udp, 1, 64);
        iface.inject_rx(RxPacket {
            dst: iface.mac(),
            src: MacAddr::new([0x02, 0, 0, 0, 0, 2]),
            ethertype: eth::ethertype::IPV4,
            payload: ip,
        });
        assert!(matches!(
            syscall_net(
                &table,
                NetSyscall::PollRx,
                SocketId(0),
                Port::new(0),
                &mut []
            )
            .unwrap(),
            NetResult::Count(n) if n > 0
        ));
        let mut buf = [0u8; 32];
        let result = syscall_net(&table, NetSyscall::RecvFrom, id, Port::new(0), &mut buf).unwrap();
        assert_eq!(
            result,
            NetResult::Datagram {
                len: 2,
                src,
                port: Port::new(5060)
            }
        );
        assert_eq!(&buf[..2], b"ok");
    }

    #[test]
    fn syscall_send_to_emits_valid_udp() {
        let (table, iface) = mk();
        let id = open(&table, SocketKind::Udp);
        syscall_net(&table, NetSyscall::Bind, id, Port::new(5555), &mut []).unwrap();
        let dst = Ipv4Addr::new([10, 0, 0, 1]);
        let mut payload = *b"payload";
        let result = syscall_net(
            &table,
            NetSyscall::SendTo {
                addr: dst,
                port: Port::new(9),
            },
            id,
            Port::new(0),
            &mut payload,
        )
        .unwrap();
        assert_eq!(result, NetResult::Count(7));
        let tx = iface.drain_tx();
        assert_eq!(tx.len(), 1);
        let packet = proto::parse_ipv4(&tx[0].payload).unwrap();
        let udp = proto::parse_udp_checked(packet.payload, packet.src, packet.dst).unwrap();
        assert_eq!(udp.payload, b"payload");
    }

    #[test]
    fn arp_request_gets_reply_and_learns_neighbour() {
        let (table, iface) = mk();
        let peer_mac = MacAddr::new([0x02, 0, 0, 0, 0, 9]);
        let peer_ip = Ipv4Addr::new([10, 0, 0, 1]);
        let request = arp::build_arp(
            arp::OP_REQUEST,
            peer_mac,
            peer_ip,
            MacAddr::new([0; 6]),
            iface.ipv4(),
        );
        iface.inject_rx(RxPacket {
            dst: MacAddr::BROADCAST,
            src: peer_mac,
            ethertype: eth::ethertype::ARP,
            payload: request,
        });
        syscall_net(
            &table,
            NetSyscall::PollRx,
            SocketId(0),
            Port::new(0),
            &mut [],
        )
        .unwrap();
        assert_eq!(table.neighbors.lookup(peer_ip), Some(peer_mac));
        let tx = iface.drain_tx();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0].ethertype, Some(eth::ethertype::ARP));
        let reply = arp::parse_arp(&tx[0].payload).unwrap();
        assert_eq!(reply.operation, arp::OP_REPLY);
        assert_eq!(reply.sender_ip, iface.ipv4());
        assert_eq!(reply.target_ip, peer_ip);
    }
}
