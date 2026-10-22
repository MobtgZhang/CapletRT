//! ICMPv4 echo request / echo reply handling.

use crate::addr::Ipv4Addr;

/// ICMP type: echo reply.
pub const ECHO_REPLY: u8 = 0;
/// ICMP type: echo request.
pub const ECHO_REQUEST: u8 = 8;

/// Internet checksum (RFC 1071) used by ICMP and IPv4.
pub fn checksum(buf: &[u8]) -> u16 {
    finish_checksum(ones_sum(buf))
}

pub(crate) fn ones_sum(buf: &[u8]) -> u32 {
    let mut sum = 0u32;
    let mut chunks = buf.chunks_exact(2);
    for word in &mut chunks {
        sum = sum.wrapping_add(u16::from_be_bytes([word[0], word[1]]) as u32);
    }
    let rest = chunks.remainder();
    if let Some(&byte) = rest.first() {
        sum = sum.wrapping_add((byte as u32) << 8);
    }
    sum
}

pub(crate) fn finish_checksum(mut sum: u32) -> u16 {
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Whether an ICMP packet is structurally valid (length and checksum).
pub fn is_valid(pkt: &[u8]) -> bool {
    pkt.len() >= 8 && checksum(pkt) == 0
}

/// Whether this is a valid ICMP echo request.
pub fn is_echo_request(pkt: &[u8]) -> bool {
    is_valid(pkt) && pkt[0] == ECHO_REQUEST && pkt[1] == 0
}

/// Whether this is a valid ICMP echo reply.
pub fn is_echo_reply(pkt: &[u8]) -> bool {
    is_valid(pkt) && pkt[0] == ECHO_REPLY && pkt[1] == 0
}

/// Decide whether an ICMP echo request is addressed to `dst`, where
/// `local` is this interface's address. The destination address is not
/// carried in the ICMP payload, so both IP-level values are required.
pub fn is_echo_request_for(pkt: &[u8], dst: Ipv4Addr, local: Ipv4Addr) -> bool {
    is_echo_request(pkt) && (local.is_unspecified() || dst == local || dst.is_broadcast())
}

/// Backwards-compatible helper. It validates the ICMP packet and rejects
/// an unspecified local address; callers that already have the IPv4
/// destination should use [`is_echo_request_for`].
pub fn is_echo_request_to(pkt: &[u8], addr: Ipv4Addr) -> bool {
    !addr.is_unspecified() && is_echo_request(pkt)
}

/// Build an ICMP echo reply from a request. Returns an empty vector for
/// a malformed request.
pub fn build_echo_reply(req: &[u8]) -> alloc::vec::Vec<u8> {
    if req.len() < 8 || req[0] != ECHO_REQUEST {
        return alloc::vec::Vec::new();
    }
    let mut out = alloc::vec::Vec::with_capacity(req.len());
    out.push(ECHO_REPLY);
    out.push(0);
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&req[4..]);
    let sum = checksum(&out);
    out[2..4].copy_from_slice(&sum.to_be_bytes());
    out
}

/// Build an ICMP echo request with the supplied identifier, sequence and
/// payload.
pub fn build_echo_request(id: u16, seq: u16, payload: &[u8]) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::with_capacity(8 + payload.len());
    out.push(ECHO_REQUEST);
    out.push(0);
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(payload);
    let sum = checksum(&out);
    out[2..4].copy_from_slice(&sum.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_basic() {
        let v = [0x45, 0x00, 0x00, 0x3c];
        assert_eq!(checksum(&v), 0xBAC3);
    }

    #[test]
    fn echo_request_and_reply_round_trip() {
        let req = build_echo_request(0x1234, 7, &[0xde, 0xad, 0xbe, 0xef]);
        assert!(is_echo_request(&req));
        assert!(!is_echo_request_to(&req, Ipv4Addr::UNSPECIFIED));
        let local = Ipv4Addr::new([10, 0, 0, 2]);
        assert!(is_echo_request_for(&req, local, local));
        let reply = build_echo_reply(&req);
        assert!(is_echo_reply(&reply));
        assert_eq!(&reply[4..], &req[4..]);
    }

    #[test]
    fn malformed_packets_are_rejected() {
        assert!(build_echo_reply(&[8, 0, 0, 0]).is_empty());
        assert!(!is_echo_request(&[8, 0, 0, 0, 1, 2, 3, 4]));
    }
}
