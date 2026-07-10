//! Ethernet frame encoding / decoding.

use crate::addr::MacAddr;

/// Decoded Ethernet frame.
#[derive(Debug, Clone)]
pub struct EthernetFrame<'a> {
    /// Destination MAC.
    pub dst: MacAddr,
    /// Source MAC.
    pub src: MacAddr,
    /// EtherType (big-endian u16).
    pub ethertype: u16,
    /// Payload after the 14-byte header.
    pub payload: &'a [u8],
}

/// Parse an Ethernet frame. Returns `None` if the buffer is
/// shorter than 14 bytes.
pub fn parse_eth(buf: &[u8]) -> Option<EthernetFrame<'_>> {
    if buf.len() < 14 {
        return None;
    }
    let dst = MacAddr::new([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5]]);
    let src = MacAddr::new([buf[6], buf[7], buf[8], buf[9], buf[10], buf[11]]);
    let ethertype = u16::from_be_bytes([buf[12], buf[13]]);
    Some(EthernetFrame {
        dst,
        src,
        ethertype,
        payload: &buf[14..],
    })
}

/// Common EtherTypes.
pub mod ethertype {
    /// IPv4.
    pub const IPV4: u16 = 0x0800;
    /// ARP.
    pub const ARP: u16 = 0x0806;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_eth_basic() {
        let mut buf = [0u8; 18];
        buf[0..6].copy_from_slice(&[0xff; 6]);
        buf[6..12].copy_from_slice(&[0x02, 0, 0, 0, 0, 1]);
        buf[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
        buf[14..18].copy_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]);
        let f = parse_eth(&buf).unwrap();
        assert_eq!(f.dst, MacAddr::BROADCAST);
        assert_eq!(f.ethertype, ethertype::IPV4);
        assert_eq!(f.payload, &[0xaa, 0xbb, 0xcc, 0xdd]);
    }
}