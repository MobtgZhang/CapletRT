//! ICMP — minimal echo-request / echo-reply.

use crate::addr::Ipv4Addr;

const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_ECHO_REQUEST: u8 = 8;

/// Build an ICMP echo reply from a request.
pub fn build_echo_reply(req: &[u8]) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::with_capacity(req.len());
    out.push(ICMP_ECHO_REPLY);
    out.push(req[1]); // code
    out.extend_from_slice(&[0, 0]); // checksum placeholder
    out.extend_from_slice(&req[4..]);
    let sum = checksum(&out);
    out[2..4].copy_from_slice(&sum.to_be_bytes());
    out
}

/// ICMP checksum — 16-bit one's complement of the one's
/// complement sum of 16-bit words.
pub fn checksum(buf: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < buf.len() {
        let word = u16::from_be_bytes([buf[i], buf[i + 1]]) as u32;
        sum = sum.wrapping_add(word);
        i += 2;
    }
    if i < buf.len() {
        sum = sum.wrapping_add((buf[i] as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Decide whether the ICMP packet is a valid echo request
/// addressed to `addr`.
pub fn is_echo_request_to(pkt: &[u8], _addr: Ipv4Addr) -> bool {
    pkt.len() >= 8 && pkt[0] == ICMP_ECHO_REQUEST
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_basic() {
        // Known vector: bytes 0x45 0x00 ... (from RFC 1071)
        let v = [0x45, 0x00, 0x00, 0x3c];
        // 0x4500 + 0x003c = 0x453c, ones complement = 0xBAC3
        assert_eq!(checksum(&v), 0xBAC3);
    }

    #[test]
    fn echo_reply_basic() {
        let req = [8, 0, 0, 0, 0xde, 0xad, 0xbe, 0xef];
        let reply = build_echo_reply(&req);
        assert_eq!(reply[0], ICMP_ECHO_REPLY);
        // Roundtrip: bytes 4.. should be the original id+seq
        assert_eq!(&reply[4..], &req[4..]);
    }
}