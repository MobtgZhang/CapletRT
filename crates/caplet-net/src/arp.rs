//! ARP (Ethernet/IPv4) packet handling and neighbour cache.

use alloc::collections::BTreeMap;
use caplet_sync::SpinLock;

use crate::addr::{Ipv4Addr, MacAddr};

/// ARP hardware type for Ethernet.
pub const HTYPE_ETHERNET: u16 = 1;
/// ARP protocol type for IPv4.
pub const PTYPE_IPV4: u16 = 0x0800;
/// Operation: request.
pub const OP_REQUEST: u16 = 1;
/// Operation: reply.
pub const OP_REPLY: u16 = 2;

/// A decoded ARP packet for Ethernet/IPv4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArpPacket {
    /// Operation (request or reply).
    pub operation: u16,
    /// Sender hardware address.
    pub sender_mac: MacAddr,
    /// Sender protocol address.
    pub sender_ip: Ipv4Addr,
    /// Target hardware address.
    pub target_mac: MacAddr,
    /// Target protocol address.
    pub target_ip: Ipv4Addr,
}

/// Parse an Ethernet/IPv4 ARP packet (28 bytes).
pub fn parse_arp(buf: &[u8]) -> Option<ArpPacket> {
    if buf.len() < 28 {
        return None;
    }
    if u16::from_be_bytes([buf[0], buf[1]]) != HTYPE_ETHERNET
        || u16::from_be_bytes([buf[2], buf[3]]) != PTYPE_IPV4
        || buf[4] != 6
        || buf[5] != 4
    {
        return None;
    }
    Some(ArpPacket {
        operation: u16::from_be_bytes([buf[6], buf[7]]),
        sender_mac: MacAddr::new([buf[8], buf[9], buf[10], buf[11], buf[12], buf[13]]),
        sender_ip: Ipv4Addr::new([buf[14], buf[15], buf[16], buf[17]]),
        target_mac: MacAddr::new([buf[18], buf[19], buf[20], buf[21], buf[22], buf[23]]),
        target_ip: Ipv4Addr::new([buf[24], buf[25], buf[26], buf[27]]),
    })
}

/// Build an ARP request or reply.
pub fn build_arp(
    operation: u16,
    sender_mac: MacAddr,
    sender_ip: Ipv4Addr,
    target_mac: MacAddr,
    target_ip: Ipv4Addr,
) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::with_capacity(28);
    out.extend_from_slice(&HTYPE_ETHERNET.to_be_bytes());
    out.extend_from_slice(&PTYPE_IPV4.to_be_bytes());
    out.push(6);
    out.push(4);
    out.extend_from_slice(&operation.to_be_bytes());
    out.extend_from_slice(sender_mac.octets());
    out.extend_from_slice(sender_ip.octets());
    out.extend_from_slice(target_mac.octets());
    out.extend_from_slice(target_ip.octets());
    out
}

/// A small bounded ARP cache. Entries are refreshed by replies and
/// can be explicitly invalidated.
pub struct ArpCache {
    entries: SpinLock<BTreeMap<Ipv4Addr, MacAddr>>,
}

impl Default for ArpCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ArpCache {
    /// Create an empty cache.
    pub const fn new() -> Self {
        Self {
            entries: SpinLock::new(BTreeMap::new()),
        }
    }

    /// Insert or refresh a mapping.
    pub fn insert(&self, ip: Ipv4Addr, mac: MacAddr) {
        self.entries.lock().insert(ip, mac);
    }

    /// Look up a mapping.
    pub fn lookup(&self, ip: Ipv4Addr) -> Option<MacAddr> {
        self.entries.lock().get(&ip).copied()
    }

    /// Remove a mapping.
    pub fn remove(&self, ip: Ipv4Addr) {
        self.entries.lock().remove(&ip);
    }

    /// Observe an ARP packet, learning the sender mapping.
    pub fn observe(&self, pkt: &ArpPacket) {
        if !pkt.sender_ip.is_unspecified() {
            self.insert(pkt.sender_ip, pkt.sender_mac);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arp_round_trip() {
        let sender_mac = MacAddr::new([2, 0, 0, 0, 0, 1]);
        let sender_ip = Ipv4Addr::new([10, 0, 0, 1]);
        let target_ip = Ipv4Addr::new([10, 0, 0, 2]);
        let raw = build_arp(
            OP_REQUEST,
            sender_mac,
            sender_ip,
            MacAddr::BROADCAST,
            target_ip,
        );
        let pkt = parse_arp(&raw).unwrap();
        assert_eq!(pkt.operation, OP_REQUEST);
        assert_eq!(pkt.sender_mac, sender_mac);
        assert_eq!(pkt.target_ip, target_ip);
    }

    #[test]
    fn cache_learns_sender() {
        let cache = ArpCache::new();
        let pkt = ArpPacket {
            operation: OP_REPLY,
            sender_mac: MacAddr::new([2, 0, 0, 0, 0, 2]),
            sender_ip: Ipv4Addr::new([192, 168, 0, 2]),
            target_mac: MacAddr::new([2, 0, 0, 0, 0, 1]),
            target_ip: Ipv4Addr::new([192, 168, 0, 1]),
        };
        cache.observe(&pkt);
        assert_eq!(cache.lookup(pkt.sender_ip), Some(pkt.sender_mac));
    }
}
