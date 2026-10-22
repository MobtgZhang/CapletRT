//! IPv4, UDP, ICMP and TCP wire-format helpers and inbound dispatch.

use alloc::vec::Vec;

use crate::addr::{Ipv4Addr, MacAddr, Port};
use crate::icmp;
use crate::interface::{NetworkInterface, TxPacket};
use crate::socket::{SocketError, SocketTable};

/// IP protocol number for ICMP.
pub const PROTO_ICMP: u8 = 1;
/// IP protocol number for TCP.
pub const PROTO_TCP: u8 = 6;
/// IP protocol number for UDP.
pub const PROTO_UDP: u8 = 17;

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

impl IpProto {
    /// Wire protocol number.
    pub const fn number(self) -> u8 {
        match self {
            Self::Icmp => PROTO_ICMP,
            Self::Tcp => PROTO_TCP,
            Self::Udp => PROTO_UDP,
        }
    }

    /// Decode a protocol number.
    pub const fn from_number(value: u8) -> Option<Self> {
        match value {
            PROTO_ICMP => Some(Self::Icmp),
            PROTO_TCP => Some(Self::Tcp),
            PROTO_UDP => Some(Self::Udp),
            _ => None,
        }
    }
}

/// Decoded IPv4 packet with a non-fragmented payload.
#[derive(Debug)]
pub struct IpPacket<'a> {
    /// Source IP.
    pub src: Ipv4Addr,
    /// Destination IP.
    pub dst: Ipv4Addr,
    /// Protocol.
    pub proto: IpProto,
    /// Time-to-live.
    pub ttl: u8,
    /// Header length in bytes (including options).
    pub header_len: usize,
    /// Total packet length from the IP header.
    pub total_len: usize,
    /// L4 payload.
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

/// Parse and validate a complete, unfragmented IPv4 packet.
pub fn parse_ipv4(buf: &[u8]) -> Option<IpPacket<'_>> {
    if buf.len() < 20 {
        return None;
    }
    let version_ihl = buf[0];
    if version_ihl >> 4 != 4 {
        return None;
    }
    let header_len = usize::from(version_ihl & 0x0f) * 4;
    if header_len < 20 || header_len > buf.len() {
        return None;
    }
    let total_len = usize::from(u16::from_be_bytes([buf[2], buf[3]]));
    if total_len < header_len || total_len > buf.len() {
        return None;
    }
    if icmp::checksum(&buf[..header_len]) != 0 {
        return None;
    }
    let fragment = u16::from_be_bytes([buf[6], buf[7]]);
    if fragment & 0x3fff != 0 {
        // No reassembly buffer: ignore fragments rather than treating a
        // fragment as a complete datagram.
        return None;
    }
    let proto = IpProto::from_number(buf[9])?;
    Some(IpPacket {
        src: Ipv4Addr::new([buf[12], buf[13], buf[14], buf[15]]),
        dst: Ipv4Addr::new([buf[16], buf[17], buf[18], buf[19]]),
        proto,
        ttl: buf[8],
        header_len,
        total_len,
        payload: &buf[header_len..total_len],
    })
}

/// Build an IPv4 packet with a 20-byte header.
pub fn build_ipv4(
    src: Ipv4Addr,
    dst: Ipv4Addr,
    proto: IpProto,
    payload: &[u8],
    identification: u16,
    ttl: u8,
) -> Vec<u8> {
    let total = 20usize.saturating_add(payload.len());
    let mut out = Vec::with_capacity(total);
    out.push(0x45);
    out.push(0);
    out.extend_from_slice(&(total as u16).to_be_bytes());
    out.extend_from_slice(&identification.to_be_bytes());
    out.extend_from_slice(&0x4000u16.to_be_bytes());
    out.push(ttl);
    out.push(proto.number());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(src.octets());
    out.extend_from_slice(dst.octets());
    let sum = icmp::checksum(&out);
    out[10..12].copy_from_slice(&sum.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Parse a UDP datagram, validating its length field.
pub fn parse_udp(buf: &[u8]) -> Option<UdpDatagram<'_>> {
    if buf.len() < 8 {
        return None;
    }
    let length = usize::from(u16::from_be_bytes([buf[4], buf[5]]));
    if length < 8 || length > buf.len() {
        return None;
    }
    Some(UdpDatagram {
        src_port: Port(u16::from_be_bytes([buf[0], buf[1]])),
        dst_port: Port(u16::from_be_bytes([buf[2], buf[3]])),
        payload: &buf[8..length],
    })
}

/// Parse UDP and validate the checksum against the IPv4 pseudo-header.
pub fn parse_udp_checked(buf: &[u8], src: Ipv4Addr, dst: Ipv4Addr) -> Option<UdpDatagram<'_>> {
    let datagram = parse_udp(buf)?;
    let length = usize::from(u16::from_be_bytes([buf[4], buf[5]]));
    let checksum = u16::from_be_bytes([buf[6], buf[7]]);
    if checksum != 0 {
        let sum = udp_checksum_sum(src, dst, &buf[..length]);
        if icmp::finish_checksum(sum) != 0 {
            return None;
        }
    }
    Some(datagram)
}

/// Build a UDP datagram including a valid pseudo-header checksum.
pub fn build_udp(
    src: Ipv4Addr,
    dst: Ipv4Addr,
    src_port: Port,
    dst_port: Port,
    payload: &[u8],
) -> Vec<u8> {
    let length = 8usize.saturating_add(payload.len());
    let mut out = Vec::with_capacity(length);
    out.extend_from_slice(&src_port.0.to_be_bytes());
    out.extend_from_slice(&dst_port.0.to_be_bytes());
    out.extend_from_slice(&(length as u16).to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(payload);
    let sum = icmp::finish_checksum(udp_checksum_sum(src, dst, &out));
    let sum = if sum == 0 { 0xffff } else { sum };
    out[6..8].copy_from_slice(&sum.to_be_bytes());
    out
}

fn udp_checksum_sum(src: Ipv4Addr, dst: Ipv4Addr, udp: &[u8]) -> u32 {
    let mut sum = icmp::ones_sum(src.octets());
    sum = sum.wrapping_add(icmp::ones_sum(dst.octets()));
    sum = sum.wrapping_add(u32::from(PROTO_UDP));
    sum = sum.wrapping_add(udp.len() as u32);
    sum.wrapping_add(icmp::ones_sum(udp))
}

/// Dispatch an inbound IP packet. `reply_mac` is the source MAC of the
/// received Ethernet frame, used for ICMP replies and TCP handshakes.
pub fn dispatch_inbound<I: NetworkInterface + 'static>(
    table: &SocketTable<I>,
    pkt: &IpPacket<'_>,
    reply_mac: MacAddr,
) -> Result<(), SocketError> {
    match pkt.proto {
        IpProto::Udp => {
            let Some(udp) = parse_udp_checked(pkt.payload, pkt.src, pkt.dst) else {
                return Ok(());
            };
            if let Some(id) = table.find_udp(udp.dst_port, Some((pkt.src, udp.src_port))) {
                table.push_rx_from(id, pkt.src, udp.src_port, udp.payload.to_vec())?;
            }
            Ok(())
        }
        IpProto::Tcp => table.handle_tcp(pkt, reply_mac),
        IpProto::Icmp => {
            if icmp::is_echo_request_for(pkt.payload, pkt.dst, table.iface.ipv4()) {
                let reply = icmp::build_echo_reply(pkt.payload);
                if !reply.is_empty() {
                    let local = table.iface.ipv4();
                    if !local.is_unspecified() {
                        let bytes = build_ipv4(local, pkt.src, IpProto::Icmp, &reply, 0, 64);
                        table
                            .iface
                            .transmit(TxPacket {
                                dst: reply_mac,
                                payload: bytes,
                                ethertype: None,
                            })
                            .map_err(|_| SocketError::NotReady)?;
                    }
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn udp_packet() -> Vec<u8> {
        let src = Ipv4Addr::new([10, 0, 0, 1]);
        let dst = Ipv4Addr::new([10, 0, 0, 2]);
        let udp = build_udp(src, dst, Port::new(4242), Port::new(5353), &[0xde, 0xad]);
        build_ipv4(src, dst, IpProto::Udp, &udp, 7, 64)
    }

    #[test]
    fn ipv4_round_trip_validates_checksum_and_length() {
        let raw = udp_packet();
        let pkt = parse_ipv4(&raw).unwrap();
        assert_eq!(pkt.proto, IpProto::Udp);
        assert_eq!(pkt.src, Ipv4Addr::new([10, 0, 0, 1]));
        assert_eq!(pkt.total_len, raw.len());
        assert!(parse_ipv4(&raw[..raw.len() - 1]).is_none());
        let mut bad = raw.clone();
        bad[10] ^= 1;
        assert!(parse_ipv4(&bad).is_none());
    }

    #[test]
    fn udp_pseudo_checksum_round_trip() {
        let raw = udp_packet();
        let pkt = parse_ipv4(&raw).unwrap();
        let udp = parse_udp_checked(pkt.payload, pkt.src, pkt.dst).unwrap();
        assert_eq!(udp.src_port, Port::new(4242));
        assert_eq!(udp.dst_port, Port::new(5353));
        assert_eq!(udp.payload, &[0xde, 0xad]);
        let mut bad = pkt.payload.to_vec();
        bad[8] ^= 1;
        assert!(parse_udp_checked(&bad, pkt.src, pkt.dst).is_none());
    }

    #[test]
    fn oversized_total_length_is_rejected() {
        let mut raw = udp_packet();
        let announced = raw.len() as u16 + 1;
        raw[2..4].copy_from_slice(&announced.to_be_bytes());
        raw[10..12].fill(0);
        let sum = icmp::checksum(&raw[..20]);
        raw[10..12].copy_from_slice(&sum.to_be_bytes());
        assert!(parse_ipv4(&raw).is_none());
    }
}
