//! Protocol-layer dispatch: IP, ICMP, UDP, TCP, DHCP, DNS.
//!
//! For Phase 7.1 this module is mostly stubs that route
//! incoming frames to the right socket table row.

use crate::addr::{Ipv4Addr, Port};
use crate::interface::NetworkInterface;
use crate::socket::{SocketError, SocketId, SocketKind, SocketTable};

/// Top-level network protocol identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpProto {
    /// ICMP.
    Icmp,
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
}

/// Decoded IP header.
#[derive(Debug)]
pub struct IpPacket<'a> {
    /// Source IP.
    pub src: Ipv4Addr,
    /// Destination IP.
    pub dst: Ipv4Addr,
    /// Protocol.
    pub proto: IpProto,
    /// Payload after the IP header.
    pub payload: &'a [u8],
}

/// Decoded UDP datagram.
#[derive(Debug)]
pub struct UdpDatagram<'a> {
    /// Source port.
    pub src_port: Port,
    /// Destination port.
    pub dst_port: Port,
    /// Payload after the UDP header.
    pub payload: &'a [u8],
}

/// Parse the smallest possible IPv4 header. Returns
/// `None` if the buffer is too short.
pub fn parse_ipv4(buf: &[u8]) -> Option<IpPacket<'_>> {
    if buf.len() < 20 {
        return None;
    }
    let version_ihl = buf[0];
    if version_ihl >> 4 != 4 {
        return None;
    }
    let ihl = (version_ihl & 0x0f) as usize * 4;
    if buf.len() < ihl {
        return None;
    }
    let proto = match buf[9] {
        1 => IpProto::Icmp,
        6 => IpProto::Tcp,
        17 => IpProto::Udp,
        _ => return None,
    };
    let src = Ipv4Addr::new([buf[12], buf[13], buf[14], buf[15]]);
    let dst = Ipv4Addr::new([buf[16], buf[17], buf[18], buf[19]]);
    Some(IpPacket {
        src,
        dst,
        proto,
        payload: &buf[ihl..],
    })
}

/// Parse a UDP header (8 bytes minimum).
pub fn parse_udp(buf: &[u8]) -> Option<UdpDatagram<'_>> {
    if buf.len() < 8 {
        return None;
    }
    let src_port = Port(u16::from_be_bytes([buf[0], buf[1]]));
    let dst_port = Port(u16::from_be_bytes([buf[2], buf[3]]));
    let length = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    if buf.len() < length || length < 8 {
        return None;
    }
    Some(UdpDatagram {
        src_port,
        dst_port,
        payload: &buf[8..length],
    })
}

/// Dispatch an inbound IP packet to the socket table.
pub fn dispatch_inbound<I: NetworkInterface + 'static>(
    table: &SocketTable<I>,
    pkt: &IpPacket<'_>,
) -> Result<(), SocketError> {
    match pkt.proto {
        IpProto::Udp => {
            let udp = match parse_udp(pkt.payload) {
                Some(u) => u,
                None => return Ok(()),
            };
            // Find a UDP socket bound to the destination port.
            let sockets = &table.sockets;
            let guard = sockets.lock();
            let target_id = guard
                .iter()
                .find(|(_, s)| {
                    s.kind == SocketKind::Udp
                        && s.local_port == udp.dst_port
                        && !s.closing
                })
                .map(|(id, _)| SocketId(*id));
            drop(guard);
            if let Some(id) = target_id {
                table.push_rx(id, udp.payload.to_vec())?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_min_ipv4_header() {
        // Version=4, IHL=5 (20 bytes), proto=17 (UDP)
        let mut hdr = [0u8; 20];
        hdr[0] = 0x45;
        hdr[9] = 17;
        hdr[12..16].copy_from_slice(&[10, 0, 0, 1]);
        hdr[16..20].copy_from_slice(&[10, 0, 0, 2]);
        let pkt = parse_ipv4(&hdr).unwrap();
        assert_eq!(pkt.proto, IpProto::Udp);
        assert_eq!(pkt.src, Ipv4Addr::new([10, 0, 0, 1]));
    }

    #[test]
    fn parse_udp_basic() {
        // Total length = 8 (UDP header) + 4 (payload) = 12.
        let length: u16 = 12;
        let mut hdr = [0u8; 8];
        hdr[0..2].copy_from_slice(&4242u16.to_be_bytes());
        hdr[2..4].copy_from_slice(&5353u16.to_be_bytes());
        hdr[4..6].copy_from_slice(&length.to_be_bytes());
        let payload = [0xde, 0xad, 0xbe, 0xef];
        let mut full: alloc::vec::Vec<u8> = hdr.to_vec();
        full.extend_from_slice(&payload);
        let d = parse_udp(full.as_slice()).unwrap();
        assert_eq!(d.src_port, Port::new(4242));
        assert_eq!(d.dst_port, Port::new(5353));
        assert_eq!(d.payload, &payload);
    }
}