//! DNS client: wire encoding, response parsing, TTL cache and a small
//! synchronous UDP resolver with multiple-server fallback.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::addr::{Ipv4Addr, Port};
use crate::eth;
use crate::interface::NetworkInterface;
use crate::proto;
use crate::socket::{SocketError, SocketKind, SocketTable};

/// DNS record type A.
pub const TYPE_A: u16 = 1;
/// DNS record type AAAA.
pub const TYPE_AAAA: u16 = 28;
/// DNS class IN.
pub const CLASS_IN: u16 = 1;
/// DNS server port.
pub const DNS_PORT: u16 = 53;

/// Errors reported by the resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
    /// The hostname is empty, has an empty/oversized label, or is too long.
    InvalidName,
    /// The response is structurally invalid or has the wrong transaction id.
    InvalidMessage,
    /// The server returned a non-zero RCODE.
    ServerFailure(u8),
    /// No address record of the requested family was returned.
    NoRecords,
    /// No configured server answered during the polling window.
    Timeout,
    /// The socket/network layer failed.
    Network,
}

impl From<SocketError> for DnsError {
    fn from(_: SocketError) -> Self {
        Self::Network
    }
}

/// Query type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    /// IPv4 address record.
    A,
    /// IPv6 address record.
    Aaaa,
}

impl QueryType {
    /// Wire value.
    pub const fn value(self) -> u16 {
        match self {
            Self::A => TYPE_A,
            Self::Aaaa => TYPE_AAAA,
        }
    }
}

/// A parsed resource record payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsRdata {
    /// IPv4 address.
    A(Ipv4Addr),
    /// IPv6 address bytes.
    Aaaa([u8; 16]),
    /// Unknown record, preserved verbatim.
    Other(Vec<u8>),
}

/// A resource record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsRecord {
    /// Owner name.
    pub name: String,
    /// Record type.
    pub record_type: u16,
    /// Record class.
    pub class: u16,
    /// Time to live in seconds.
    pub ttl: u32,
    /// Record data.
    pub data: DnsRdata,
}

/// Parsed DNS response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsResponse {
    /// Transaction id.
    pub id: u16,
    /// Authoritative answer flag.
    pub authoritative: bool,
    /// Truncation flag.
    pub truncated: bool,
    /// Response code.
    pub rcode: u8,
    /// Answer records.
    pub answers: Vec<DnsRecord>,
}

/// Decode a DNS name from wire format. Compression pointers must point
/// backwards, which guarantees termination for malformed/cyclic input.
pub fn decode_name(buf: &[u8], offset: usize) -> Option<(String, usize)> {
    let mut i = offset;
    let mut out = String::new();
    let mut jumped = false;
    let mut end = offset;
    let mut steps = 0usize;
    loop {
        if i >= buf.len() || steps > 128 || out.len() > 253 {
            return None;
        }
        steps += 1;
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
            let ptr = (((len & 0x3f) as usize) << 8) | usize::from(buf[i + 1]);
            if ptr >= i {
                return None;
            }
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
        let next = i.checked_add(usize::from(len))?;
        if next > buf.len() {
            return None;
        }
        if !out.is_empty() {
            out.push('.');
        }
        let label = core::str::from_utf8(&buf[i..next]).ok()?;
        out.push_str(label);
        i = next;
    }
}

/// Encode a name into DNS wire format. Returns `None` for an empty name,
/// an empty label, a label longer than 63 bytes, or a name over 253 bytes.
pub fn encode_name(name: &str) -> Option<Vec<u8>> {
    if name.is_empty() || name.len() > 253 {
        return None;
    }
    let mut out = Vec::with_capacity(name.len() + 2);
    let trimmed = name.strip_suffix('.').unwrap_or(name);
    if trimmed.is_empty() {
        return Some(alloc::vec![0]);
    }
    for label in trimmed.split('.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    if out.len() > 255 {
        return None;
    }
    Some(out)
}

/// Build a recursive DNS query. `id` is the transaction identifier.
pub fn build_query(name: &str, query_type: QueryType, id: u16) -> Option<Vec<u8>> {
    let qname = encode_name(name)?;
    let mut out = Vec::with_capacity(12 + qname.len() + 4);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&0x0100u16.to_be_bytes()); // recursion desired
    out.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // AN/NS/AR
    out.extend_from_slice(&qname);
    out.extend_from_slice(&query_type.value().to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    Some(out)
}

fn read_u16(buf: &[u8], offset: usize) -> Option<u16> {
    let bytes = buf.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32(buf: &[u8], offset: usize) -> Option<u32> {
    let bytes = buf.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Parse a DNS response, including compressed answer names.
pub fn parse_response(buf: &[u8]) -> Result<DnsResponse, DnsError> {
    if buf.len() < 12 {
        return Err(DnsError::InvalidMessage);
    }
    let id = read_u16(buf, 0).ok_or(DnsError::InvalidMessage)?;
    let flags = read_u16(buf, 2).ok_or(DnsError::InvalidMessage)?;
    if flags & 0x8000 == 0 {
        return Err(DnsError::InvalidMessage);
    }
    let qdcount = usize::from(read_u16(buf, 4).ok_or(DnsError::InvalidMessage)?);
    let ancount = usize::from(read_u16(buf, 6).ok_or(DnsError::InvalidMessage)?);
    let rcode = (flags & 0x000f) as u8;
    let mut offset = 12usize;
    for _ in 0..qdcount {
        let (_, used) = decode_name(buf, offset).ok_or(DnsError::InvalidMessage)?;
        offset = offset
            .checked_add(used)
            .and_then(|v| v.checked_add(4))
            .ok_or(DnsError::InvalidMessage)?;
        if offset > buf.len() {
            return Err(DnsError::InvalidMessage);
        }
    }
    let mut answers = Vec::with_capacity(ancount);
    for _ in 0..ancount {
        let (name, used) = decode_name(buf, offset).ok_or(DnsError::InvalidMessage)?;
        offset = offset.checked_add(used).ok_or(DnsError::InvalidMessage)?;
        let record_type = read_u16(buf, offset).ok_or(DnsError::InvalidMessage)?;
        let class = read_u16(buf, offset + 2).ok_or(DnsError::InvalidMessage)?;
        let ttl = read_u32(buf, offset + 4).ok_or(DnsError::InvalidMessage)?;
        let rdlength = usize::from(read_u16(buf, offset + 8).ok_or(DnsError::InvalidMessage)?);
        offset = offset.checked_add(10).ok_or(DnsError::InvalidMessage)?;
        let end = offset
            .checked_add(rdlength)
            .ok_or(DnsError::InvalidMessage)?;
        let rdata = buf.get(offset..end).ok_or(DnsError::InvalidMessage)?;
        let data = match (record_type, rdata.len()) {
            (TYPE_A, 4) => DnsRdata::A(Ipv4Addr::new([rdata[0], rdata[1], rdata[2], rdata[3]])),
            (TYPE_AAAA, 16) => {
                let mut addr = [0u8; 16];
                addr.copy_from_slice(rdata);
                DnsRdata::Aaaa(addr)
            }
            _ => DnsRdata::Other(rdata.to_vec()),
        };
        answers.push(DnsRecord {
            name,
            record_type,
            class,
            ttl,
            data,
        });
        offset = end;
    }
    Ok(DnsResponse {
        id,
        authoritative: flags & 0x0400 != 0,
        truncated: flags & 0x0200 != 0,
        rcode,
        answers,
    })
}

/// An entry in the TTL cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    /// Addresses learned from A records.
    pub addrs: Vec<Ipv4Addr>,
    /// Time at which the entry becomes stale, in the caller's clock units.
    pub expires_at: u64,
}

/// Small DNS cache. `now` uses the same monotonic clock units as the
/// platform; seconds are the simplest choice.
#[derive(Debug, Default)]
pub struct DnsCache {
    entries: BTreeMap<String, CacheEntry>,
}

impl DnsCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a fresh address.
    pub fn lookup(&self, name: &str, now: u64) -> Option<&CacheEntry> {
        self.entries
            .get(&normalise_name(name))
            .filter(|entry| entry.expires_at > now)
    }

    /// Insert/replace an entry. A zero TTL is not cached.
    pub fn insert(&mut self, name: &str, addrs: Vec<Ipv4Addr>, ttl: u32, now: u64) {
        if addrs.is_empty() || ttl == 0 {
            return;
        }
        self.entries.insert(
            normalise_name(name),
            CacheEntry {
                addrs,
                expires_at: now.saturating_add(u64::from(ttl)),
            },
        );
    }

    /// Remove every entry.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of cached names (including possibly stale entries).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn normalise_name(name: &str) -> String {
    let trimmed = name.strip_suffix('.').unwrap_or(name);
    trimmed.to_ascii_lowercase()
}

/// A minimal synchronous resolver.
///
/// [`Self::resolve`] sends a query through a UDP socket and drains frames
/// already queued by the interface. On an event-driven system callers can
/// use [`Self::build_query`] / [`Self::handle_response`] directly and keep
/// the socket open instead.
pub struct Resolver {
    servers: Vec<Ipv4Addr>,
    cache: DnsCache,
    next_id: u16,
    /// Maximum frames inspected per server before falling back.
    pub max_polls: usize,
}

impl Resolver {
    /// Construct a resolver with an ordered server list.
    pub fn new(servers: Vec<Ipv4Addr>) -> Self {
        Self {
            servers,
            cache: DnsCache::new(),
            next_id: 0x4d2,
            max_polls: 32,
        }
    }

    /// Mutable access to the cache.
    pub fn cache(&self) -> &DnsCache {
        &self.cache
    }

    /// Configured servers.
    pub fn servers(&self) -> &[Ipv4Addr] {
        &self.servers
    }

    /// Build a query using the next transaction id.
    pub fn build_query(&mut self, name: &str) -> Result<(u16, Vec<u8>), DnsError> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let query = build_query(name, QueryType::A, id).ok_or(DnsError::InvalidName)?;
        Ok((id, query))
    }

    /// Parse a response and update the cache.
    pub fn handle_response(
        &mut self,
        name: &str,
        expected_id: u16,
        message: &[u8],
        now: u64,
    ) -> Result<Ipv4Addr, DnsError> {
        let response = parse_response(message)?;
        if response.id != expected_id {
            return Err(DnsError::InvalidMessage);
        }
        if response.rcode != 0 {
            return Err(DnsError::ServerFailure(response.rcode));
        }
        let mut addrs = Vec::new();
        let mut min_ttl = u32::MAX;
        for answer in &response.answers {
            if answer.record_type == TYPE_A && answer.class == CLASS_IN {
                if let DnsRdata::A(addr) = answer.data {
                    addrs.push(addr);
                    min_ttl = min_ttl.min(answer.ttl);
                }
            }
        }
        let Some(first) = addrs.first().copied() else {
            return Err(DnsError::NoRecords);
        };
        self.cache.insert(name, addrs, min_ttl, now);
        Ok(first)
    }

    /// Resolve a name. Cached answers are returned immediately. On a miss,
    /// each configured server is tried in order; a server contributes only
    /// frames that are already available during the polling window.
    pub fn resolve<I: NetworkInterface + 'static>(
        &mut self,
        table: &SocketTable<I>,
        name: &str,
        now: u64,
    ) -> Result<Ipv4Addr, DnsError> {
        if let Some(entry) = self.cache.lookup(name, now) {
            if let Some(addr) = entry.addrs.first() {
                return Ok(*addr);
            }
        }
        if self.servers.is_empty() {
            return Err(DnsError::Network);
        }
        let (id, query) = self.build_query(name)?;
        for server in self.servers.clone() {
            let sid = table.open(SocketKind::Udp)?;
            table.bind(sid, Port::new(0))?;
            table.send_udp(sid, server, Port::new(DNS_PORT), &query)?;
            let mut answer = None;
            for _ in 0..self.max_polls {
                let Some(frame) = table.iface.poll_rx().map_err(|_| DnsError::Network)? else {
                    break;
                };
                if frame.ethertype == eth::ethertype::IPV4 {
                    if let Some(ip) = proto::parse_ipv4(&frame.payload) {
                        table.neighbors.insert(ip.src, frame.src);
                        let _ = proto::dispatch_inbound(table, &ip, frame.src);
                    }
                }
                let mut buf = [0u8; 512];
                if let Ok((n, src, src_port)) = table.recv_from(sid, &mut buf) {
                    if src == server && src_port == Port::new(DNS_PORT) {
                        if let Ok(addr) = self.handle_response(name, id, &buf[..n], now) {
                            answer = Some(addr);
                            break;
                        }
                    }
                }
            }
            let _ = table.close(sid);
            if let Some(addr) = answer {
                return Ok(addr);
            }
        }
        Err(DnsError::Timeout)
    }
}

/// Resolve a name to an IPv4 address from a static table.
pub fn resolve_static(name: &str, table: &[(&str, Ipv4Addr)]) -> Option<Ipv4Addr> {
    table
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
        .map(|(_, addr)| *addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(id: u16, addr: [u8; 4], ttl: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&id.to_be_bytes());
        out.extend_from_slice(&0x8180u16.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&encode_name("foo.example.com").unwrap());
        out.extend_from_slice(&TYPE_A.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        // Compressed pointer to the question name at offset 12.
        out.extend_from_slice(&[0xc0, 0x0c]);
        out.extend_from_slice(&TYPE_A.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        out.extend_from_slice(&ttl.to_be_bytes());
        out.extend_from_slice(&4u16.to_be_bytes());
        out.extend_from_slice(&addr);
        out
    }

    #[test]
    fn round_trip_name_and_relative_name() {
        let enc = encode_name("foo.bar.example.com").unwrap();
        let (dec, used) = decode_name(&enc, 0).unwrap();
        assert_eq!(dec, "foo.bar.example.com");
        assert_eq!(used, enc.len());
        assert!(encode_name("foo..bar").is_none());
        assert!(encode_name(&"a".repeat(64)).is_none());
    }

    #[test]
    fn query_has_expected_header_and_question() {
        let query = build_query("broker.local", QueryType::A, 0x1234).unwrap();
        assert_eq!(&query[..2], &0x1234u16.to_be_bytes());
        assert_eq!(&query[4..6], &1u16.to_be_bytes());
        let (name, used) = decode_name(&query, 12).unwrap();
        assert_eq!(name, "broker.local");
        assert_eq!(&query[12 + used..14 + used], &TYPE_A.to_be_bytes());
    }

    #[test]
    fn parse_compressed_a_record() {
        let parsed = parse_response(&response(7, [192, 0, 2, 9], 60)).unwrap();
        assert_eq!(parsed.id, 7);
        assert_eq!(parsed.answers.len(), 1);
        assert_eq!(parsed.answers[0].name, "foo.example.com");
        assert_eq!(
            parsed.answers[0].data,
            DnsRdata::A(Ipv4Addr::new([192, 0, 2, 9]))
        );
    }

    #[test]
    fn resolver_caches_and_expires() {
        let mut resolver = Resolver::new(alloc::vec![Ipv4Addr::new([8, 8, 8, 8])]);
        let (id, _) = resolver.build_query("foo.example.com").unwrap();
        assert_eq!(
            resolver.handle_response(
                "foo.example.com",
                id,
                &response(id, [192, 0, 2, 9], 60),
                100
            ),
            Ok(Ipv4Addr::new([192, 0, 2, 9]))
        );
        assert!(resolver.cache().lookup("FOO.EXAMPLE.COM", 159).is_some());
        assert!(resolver.cache().lookup("foo.example.com", 160).is_none());
    }

    #[test]
    fn wrong_id_and_rcode_are_rejected() {
        let mut resolver = Resolver::new(Vec::new());
        assert_eq!(
            resolver.handle_response("foo.example.com", 8, &response(7, [1, 2, 3, 4], 10), 0),
            Err(DnsError::InvalidMessage)
        );
        let mut nxdomain = response(8, [1, 2, 3, 4], 10);
        nxdomain[3] = 0x83;
        assert_eq!(
            resolver.handle_response("foo.example.com", 8, &nxdomain, 0),
            Err(DnsError::ServerFailure(3))
        );
    }

    #[test]
    fn static_resolve_is_case_insensitive() {
        let table = [("broker.local", Ipv4Addr::new([10, 0, 0, 1]))];
        assert_eq!(
            resolve_static("BROKER.LOCAL", &table),
            Some(Ipv4Addr::new([10, 0, 0, 1]))
        );
        assert_eq!(resolve_static("missing.local", &table), None);
    }
}
