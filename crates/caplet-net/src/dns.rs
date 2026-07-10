//! DNS — minimal stub that models the surface a real
//! resolver needs. Encoding/decoding a name into the
//! length-prefixed wire format is enough for tests.
//!
//! ## Implementation Plan (M2.x)
//!
//! A full DNS resolver implementation should include:
//!
//! ### DNS Query Support
//! ```ignore
//! pub async fn resolve(hostname: &str) -> Result<Ipv4Addr, DnsError> {
//!     // 1. Build DNS query packet
//!     // 2. Send via UDP socket to configured DNS server
//!     // 3. Wait for response (with timeout)
//!     // 4. Parse response and extract IP
//! }
//! ```
//!
//! ### Features
//! - Recursive queries to upstream DNS server
//! - Response caching with TTL
//! - Multiple DNS server fallback
//! - EDNS0 support for large responses
//!
//! ### Cache Entry
//! ```ignore
//! struct CacheEntry {
//!     name: String,
//!     addrs: Vec<Ipv4Addr>,
//!     ttl: u32,
//!     expires_at: Instant,
//! }
//! ```

use crate::addr::Ipv4Addr;

/// Decode a DNS name from a wire-format buffer. The first
/// byte is either a label length (0-63) or a pointer marker
/// (0xc0). Returns the name and the number of bytes
/// consumed.
pub fn decode_name<'a>(buf: &'a [u8], offset: usize) -> Option<(alloc::string::String, usize)> {
    let mut i = offset;
    let mut out = alloc::string::String::new();
    let mut jumped = false;
    let mut end = offset;
    loop {
        if i >= buf.len() {
            return None;
        }
        let len = buf[i];
        if len == 0 {
            if !jumped {
                end = i + 1;
            }
            return Some((out, end - offset));
        }
        if len & 0xc0 == 0xc0 {
            if i + 1 >= buf.len() {
                return None;
            }
            let ptr = (((len & 0x3f) as usize) << 8) | (buf[i + 1] as usize);
            if !jumped {
                end = i + 2;
                jumped = true;
            }
            i = ptr;
            continue;
        }
        if len > 63 {
            return None;
        }
        i += 1;
        if i + (len as usize) > buf.len() {
            return None;
        }
        if !out.is_empty() {
            out.push('.');
        }
        // ASCII labels in DNS.
        let label = core::str::from_utf8(&buf[i..i + len as usize]).ok()?;
        out.push_str(label);
        i += len as usize;
    }
}

/// Encode a name into a DNS wire-format buffer.
pub fn encode_name(name: &str) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::new();
    for label in name.split('.') {
        if label.len() > 63 {
            continue;
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out
}

/// Resolve a name to an IPv4 address from a static table. The
/// real implementation will query the resolver; for the smoke
/// test we just do a lookup.
pub fn resolve_static(name: &str, table: &[(&str, Ipv4Addr)]) -> Option<Ipv4Addr> {
    for (n, a) in table {
        if *n == name {
            return Some(*a);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_name() {
        let enc = encode_name("foo.bar.example.com");
        let (dec, used) = decode_name(&enc, 0).unwrap();
        assert_eq!(dec, "foo.bar.example.com");
        assert_eq!(used, enc.len());
    }

    #[test]
    fn static_resolve() {
        let table = [("broker.local", Ipv4Addr::new([10, 0, 0, 1]))];
        assert_eq!(resolve_static("broker.local", &table), Some(Ipv4Addr::new([10, 0, 0, 1])));
        assert_eq!(resolve_static("missing.local", &table), None);
    }
}