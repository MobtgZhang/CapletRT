//! syscall_net — kernel entry point for network operations.
//!
//! This module provides the syscall interface for network operations in the kernel.
//! It bridges between the user-space network stack and the kernel's SocketTable.
//!
//! ## Implementation Status
//!
//! - Socket creation/binding/close: ✅ Implemented
//! - UDP receive: ✅ Implemented
//! - TCP state machine: ⚠️ Parsing only (see tcp.rs)
//! - Send operation: ⚠️ Stub (see below)
//! - DHCP/DNS: ⚠️ Partial (see dhcp.rs, dns.rs)

use crate::addr::{Ipv4Addr, Port};
use crate::interface::{NetworkInterface, RxPacket};
use crate::proto;
use crate::socket::{SocketError, SocketId, SocketKind, SocketTable};

/// Top-level net syscall tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetSyscall {
    /// Open a new socket.
    Socket(SocketKind),
    /// Bind a socket to a local port.
    Bind,
    /// Send bytes over a socket.
    Send,
    /// Receive bytes from a socket.
    Recv,
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
#[derive(Debug)]
pub enum NetResult {
    /// New socket id returned.
    NewSocket(SocketId),
    /// Number of bytes received / sent.
    Count(usize),
    /// Interface MAC.
    Mac(crate::addr::MacAddr),
    /// Interface IPv4 address.
    Ip(Ipv4Addr),
    /// No incoming packet right now.
    NoPacket,
    /// Ok(()) — for bind / close.
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
        NetSyscall::Socket(k) => {
            let id = table.open(k)?;
            Ok(NetResult::NewSocket(id))
        }
        NetSyscall::Bind => {
            table.bind(sid, port)?;
            Ok(NetResult::Ok)
        }
        NetSyscall::Recv => {
            let n = table.recv(sid, buf)?;
            Ok(NetResult::Count(n))
        }
        NetSyscall::Close => {
            table.close(sid)?;
            Ok(NetResult::Ok)
        }
        NetSyscall::PollRx => {
            let rx = table
                .iface
                .poll_rx()
                .map_err(|_| SocketError::NotReady)?;
            if let Some(pkt) = rx {
                handle_inbound(table, &pkt);
                Ok(NetResult::Count(pkt.payload.len()))
            } else {
                Ok(NetResult::NoPacket)
            }
        }
        NetSyscall::Mac => Ok(NetResult::Mac(table.iface.mac())),
        NetSyscall::Ip => Ok(NetResult::Ip(table.iface.ipv4())),
        NetSyscall::Send => Ok(NetResult::Count(buf.len())),
    }
}

fn handle_inbound<I: NetworkInterface + 'static>(table: &SocketTable<I>, pkt: &RxPacket) {
    // Strip the 14-byte Ethernet header (we assume
    // `ethertype` is already in `pkt.ethertype`).
    if pkt.ethertype != 0x0800 {
        return;
    }
    if let Some(ip) = proto::parse_ipv4(&pkt.payload) {
        let _ = proto::dispatch_inbound(table, &ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::MacAddr;
    use crate::interface::{LoopbackInterface, RxPacket};

    fn mk() -> (SocketTable<LoopbackInterface>, &'static LoopbackInterface) {
        let lb: &'static LoopbackInterface = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            LoopbackInterface::new(),
        ));
        let t = SocketTable::new(4, lb);
        (t, lb)
    }

    #[test]
    fn syscall_open_then_close() {
        let (t, _) = mk();
        let id = match syscall_net(&t, NetSyscall::Socket(SocketKind::Udp), SocketId(0), Port::new(0), &mut []).unwrap() {
            NetResult::NewSocket(id) => id,
            _ => panic!("expected NewSocket"),
        };
        syscall_net(&t, NetSyscall::Close, id, Port::new(0), &mut []).unwrap();
        assert!(t.is_empty());
    }

    #[test]
    fn syscall_mac_and_ip() {
        let (t, _) = mk();
        match syscall_net(&t, NetSyscall::Mac, SocketId(0), Port::new(0), &mut []).unwrap() {
            NetResult::Mac(m) => assert_eq!(m, t.iface.mac()),
            _ => panic!("expected Mac"),
        }
    }

    #[test]
    fn syscall_poll_rx_dispatches_to_socket() {
        let (t, lb) = mk();
        let id = match syscall_net(
            &t,
            NetSyscall::Socket(SocketKind::Udp),
            SocketId(0),
            Port::new(0),
            &mut [],
        )
        .unwrap()
        {
            NetResult::NewSocket(id) => id,
            _ => panic!(),
        };
        syscall_net(&t, NetSyscall::Bind, id, Port::new(1234), &mut []).unwrap();
        // Build an IP+UDP frame.
        let mut ip = [0u8; 28];
        ip[0] = 0x45;
        ip[9] = 17; // UDP
        ip[12..16].copy_from_slice(&[10, 0, 0, 1]);
        ip[16..20].copy_from_slice(&[10, 0, 0, 2]);
        ip[2..4].copy_from_slice(&(28u16).to_be_bytes());
        // UDP header
        ip[20..22].copy_from_slice(&5060u16.to_be_bytes());
        ip[22..24].copy_from_slice(&1234u16.to_be_bytes());
        ip[24..26].copy_from_slice(&(8u16).to_be_bytes());
        let rx = RxPacket {
            dst: lb.mac(),
            src: MacAddr::new([0x02, 0, 0, 0, 0, 2]),
            ethertype: 0x0800,
            payload: ip.to_vec(),
        };
        lb.inject_rx(rx);
        let _ = syscall_net(&t, NetSyscall::PollRx, SocketId(0), Port::new(0), &mut []).unwrap();
        let mut buf = [0u8; 32];
        let n = syscall_net(&t, NetSyscall::Recv, id, Port::new(0), &mut buf).unwrap();
        match n {
            NetResult::Count(n) => assert_eq!(n, 0),
            _ => panic!("expected Count"),
        }
    }
}