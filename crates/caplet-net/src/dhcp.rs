//! DHCPv4 client: BOOTP framing and the DISCOVER / OFFER / REQUEST /
//! ACK state machine.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::addr::Ipv4Addr;

/// UDP server port.
pub const SERVER_PORT: u16 = 67;
/// UDP client port.
pub const CLIENT_PORT: u16 = 68;
/// Size of the fixed BOOTP header, before the magic cookie.
pub const BOOTP_FIXED_LEN: usize = 236;

const MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];
const OPTION_PAD: u8 = 0;
const OPTION_SUBNET_MASK: u8 = 1;
const OPTION_ROUTER: u8 = 3;
const OPTION_DNS: u8 = 6;
const OPTION_LEASE_TIME: u8 = 51;
const OPTION_MESSAGE_TYPE: u8 = 53;
const OPTION_SERVER_ID: u8 = 54;
const OPTION_PARAMETER_LIST: u8 = 55;
const OPTION_END: u8 = 255;

static NEXT_XID: AtomicU32 = AtomicU32::new(0x1357_2468);

fn generate_xid() -> u32 {
    let sequence = NEXT_XID.fetch_add(0x9e37_79b9, Ordering::Relaxed);
    let mixed = sequence.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    if mixed == 0 {
        1
    } else {
        mixed
    }
}

/// DHCP message types.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhcpMessageType {
    /// Discover.
    Discover = 1,
    /// Offer.
    Offer = 2,
    /// Request.
    Request = 3,
    /// Decline.
    Decline = 4,
    /// Ack.
    Ack = 5,
    /// Nak.
    Nak = 6,
    /// Release.
    Release = 7,
}

impl DhcpMessageType {
    /// Decode the option-53 value.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Discover),
            2 => Some(Self::Offer),
            3 => Some(Self::Request),
            4 => Some(Self::Decline),
            5 => Some(Self::Ack),
            6 => Some(Self::Nak),
            7 => Some(Self::Release),
            _ => None,
        }
    }
}

/// DHCP state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhcpState {
    /// Init.
    Init,
    /// Selecting (DISCOVER sent, awaiting OFFER).
    Selecting,
    /// Requesting (REQUEST sent, awaiting ACK).
    Requesting,
    /// Bound — we have an address.
    Bound,
    /// Renewing.
    Renewing,
}

/// A decoded server reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DhcpReply {
    /// Message type.
    pub message_type: DhcpMessageType,
    /// Transaction id copied from the BOOTP header.
    pub xid: u32,
    /// Address offered/assigned in `yiaddr`.
    pub yiaddr: Ipv4Addr,
    /// Server identifier (option 54).
    pub server: Option<Ipv4Addr>,
    /// Subnet mask (option 1).
    pub subnet_mask: Option<Ipv4Addr>,
    /// Default router (first address of option 3).
    pub router: Option<Ipv4Addr>,
    /// DNS servers (option 6).
    pub dns_servers: Vec<Ipv4Addr>,
    /// Lease duration in seconds (option 51).
    pub lease_secs: u32,
}

/// DHCP client state holder.
#[derive(Debug)]
pub struct DhcpClient {
    /// State.
    pub state: DhcpState,
    /// Current bound address.
    pub addr: Ipv4Addr,
    /// Address most recently offered by a server (before ACK).
    pub offered_addr: Ipv4Addr,
    /// Transaction id.
    pub xid: u32,
    /// Server address.
    pub server: Option<Ipv4Addr>,
    /// Lease time in seconds.
    pub lease_secs: u32,
    /// Subnet mask learned from the lease.
    pub subnet_mask: Option<Ipv4Addr>,
    /// Default router learned from the lease.
    pub router: Option<Ipv4Addr>,
    /// DNS servers learned from the lease.
    pub dns_servers: Vec<Ipv4Addr>,
    /// Hardware address placed in `chaddr`.
    pub mac: [u8; 6],
}

impl Default for DhcpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DhcpClient {
    /// Construct a new DHCP client in the Init state.
    pub fn new() -> Self {
        Self::with_mac([0x02, 0, 0, 0, 0, 1])
    }

    /// Construct a client with an explicit MAC address.
    pub fn with_mac(mac: [u8; 6]) -> Self {
        Self {
            state: DhcpState::Init,
            addr: Ipv4Addr::UNSPECIFIED,
            offered_addr: Ipv4Addr::UNSPECIFIED,
            xid: generate_xid(),
            server: None,
            lease_secs: 0,
            subnet_mask: None,
            router: None,
            dns_servers: Vec::new(),
            mac,
        }
    }

    /// Start (or restart) discovery. The same transaction id is reused for
    /// retransmissions of this exchange.
    pub fn start(&mut self) -> Vec<u8> {
        self.state = DhcpState::Selecting;
        self.offered_addr = Ipv4Addr::UNSPECIFIED;
        self.server = None;
        if self.xid == 0 {
            self.xid = generate_xid();
        }
        self.build_discover()
    }

    /// Start a renewal with a fresh transaction id.
    pub fn start_renew(&mut self) -> Vec<u8> {
        self.xid = generate_xid();
        self.state = DhcpState::Renewing;
        self.build_discover()
    }

    /// Build a DHCP DISCOVER payload (BOOTP fixed header + options).
    pub fn build_discover(&self) -> Vec<u8> {
        self.build_message(DhcpMessageType::Discover, None, None, false)
    }

    /// Build a DHCP REQUEST for the offered address and selected server.
    pub fn build_request(&self) -> Option<Vec<u8>> {
        if self.offered_addr.is_unspecified() {
            return None;
        }
        Some(self.build_message(
            DhcpMessageType::Request,
            Some(self.offered_addr),
            self.server,
            false,
        ))
    }

    /// Build a DHCP RELEASE for a bound lease.
    pub fn build_release(&self) -> Option<Vec<u8>> {
        if self.state != DhcpState::Bound || self.server.is_none() {
            return None;
        }
        Some(self.build_message(DhcpMessageType::Release, None, self.server, true))
    }

    fn build_message(
        &self,
        message_type: DhcpMessageType,
        requested: Option<Ipv4Addr>,
        server: Option<Ipv4Addr>,
        include_client_addr: bool,
    ) -> Vec<u8> {
        let mut out = alloc::vec![0u8; BOOTP_FIXED_LEN];
        out[0] = 1; // BOOTREQUEST
        out[1] = 1; // Ethernet
        out[2] = 6; // MAC length
        out[3] = 0; // hops
        out[4..8].copy_from_slice(&self.xid.to_be_bytes());
        // Broadcast flag: the client has no usable address yet.
        out[10..12].copy_from_slice(&0x8000u16.to_be_bytes());
        if include_client_addr {
            out[12..16].copy_from_slice(self.addr.octets());
        }
        out[28..34].copy_from_slice(&self.mac);
        out.extend_from_slice(&MAGIC_COOKIE);
        out.push(OPTION_MESSAGE_TYPE);
        out.push(1);
        out.push(message_type as u8);
        if let Some(addr) = requested {
            out.push(50); // requested IP address
            out.push(4);
            out.extend_from_slice(addr.octets());
        }
        if let Some(addr) = server {
            out.push(OPTION_SERVER_ID);
            out.push(4);
            out.extend_from_slice(addr.octets());
        }
        out.push(OPTION_PARAMETER_LIST);
        out.push(4);
        out.extend_from_slice(&[
            OPTION_SUBNET_MASK,
            OPTION_ROUTER,
            OPTION_DNS,
            OPTION_LEASE_TIME,
        ]);
        out.push(OPTION_END);
        out
    }

    /// Apply a DHCP OFFER: move from Selecting to Requesting, remember the
    /// server and the offered address.
    pub fn apply_offer(&mut self, server: Ipv4Addr, offered: Ipv4Addr) {
        if self.state == DhcpState::Selecting {
            self.server = Some(server);
            self.offered_addr = offered;
            self.state = DhcpState::Requesting;
        }
    }

    /// Apply a DHCP ACK and copy the lease parameters.
    pub fn apply_ack(&mut self, addr: Ipv4Addr, lease: u32) {
        if matches!(self.state, DhcpState::Requesting | DhcpState::Renewing) {
            self.addr = addr;
            self.offered_addr = addr;
            self.lease_secs = lease;
            self.state = DhcpState::Bound;
        }
    }

    /// Reject an ACK received out of state.
    pub fn apply_nak(&mut self) {
        self.state = DhcpState::Init;
        self.addr = Ipv4Addr::UNSPECIFIED;
        self.offered_addr = Ipv4Addr::UNSPECIFIED;
        self.server = None;
        self.lease_secs = 0;
    }

    /// Feed a reply into the state machine. The transaction id must match
    /// this exchange; malformed or unrelated replies are ignored.
    pub fn handle_reply(&mut self, payload: &[u8]) -> Option<DhcpReply> {
        let reply = parse_reply(payload)?;
        if reply.xid != self.xid {
            return None;
        }
        match reply.message_type {
            DhcpMessageType::Offer => {
                let server = reply.server.unwrap_or(Ipv4Addr::UNSPECIFIED);
                self.apply_offer(server, reply.yiaddr);
            }
            DhcpMessageType::Ack => {
                if !reply.yiaddr.is_unspecified() {
                    self.subnet_mask = reply.subnet_mask;
                    self.router = reply.router;
                    self.dns_servers = reply.dns_servers.clone();
                    self.apply_ack(reply.yiaddr, reply.lease_secs);
                }
            }
            DhcpMessageType::Nak => self.apply_nak(),
            _ => {}
        }
        Some(reply)
    }
}

/// Parse a DHCP server message. Returns `None` for malformed input or a
/// packet without a message-type option.
pub fn parse_reply(buf: &[u8]) -> Option<DhcpReply> {
    if buf.len() < BOOTP_FIXED_LEN + 4 || buf[0] != 2 {
        return None;
    }
    if buf[BOOTP_FIXED_LEN..BOOTP_FIXED_LEN + 4] != MAGIC_COOKIE {
        return None;
    }
    let xid = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let yiaddr = Ipv4Addr::new([buf[16], buf[17], buf[18], buf[19]]);
    let mut message_type = None;
    let mut server = None;
    let mut subnet_mask = None;
    let mut router = None;
    let mut dns_servers = Vec::new();
    let mut lease_secs = 0u32;
    let mut offset = BOOTP_FIXED_LEN + 4;
    while offset < buf.len() {
        let code = buf[offset];
        offset += 1;
        if code == OPTION_PAD {
            continue;
        }
        if code == OPTION_END {
            break;
        }
        let len = usize::from(*buf.get(offset)?);
        offset += 1;
        let end = offset.checked_add(len)?;
        let value = buf.get(offset..end)?;
        match code {
            OPTION_MESSAGE_TYPE if len == 1 => {
                message_type = DhcpMessageType::from_u8(value[0]);
            }
            OPTION_SERVER_ID if len == 4 => {
                server = Some(Ipv4Addr::new([value[0], value[1], value[2], value[3]]));
            }
            OPTION_SUBNET_MASK if len == 4 => {
                subnet_mask = Some(Ipv4Addr::new([value[0], value[1], value[2], value[3]]));
            }
            OPTION_ROUTER if len >= 4 => {
                router = Some(Ipv4Addr::new([value[0], value[1], value[2], value[3]]));
            }
            OPTION_DNS if len % 4 == 0 => {
                for chunk in value.chunks_exact(4) {
                    dns_servers.push(Ipv4Addr::new([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            OPTION_LEASE_TIME if len == 4 => {
                lease_secs = u32::from_be_bytes([value[0], value[1], value[2], value[3]]);
            }
            _ => {}
        }
        offset = end;
    }
    Some(DhcpReply {
        message_type: message_type?,
        xid,
        yiaddr,
        server,
        subnet_mask,
        router,
        dns_servers,
        lease_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(xid: u32, server: [u8; 4], offered: [u8; 4], lease: u32) -> Vec<u8> {
        let mut out = alloc::vec![0u8; BOOTP_FIXED_LEN];
        out[0] = 2;
        out[1] = 1;
        out[2] = 6;
        out[4..8].copy_from_slice(&xid.to_be_bytes());
        out[16..20].copy_from_slice(&offered);
        out.extend_from_slice(&MAGIC_COOKIE);
        out.extend_from_slice(&[OPTION_MESSAGE_TYPE, 1, DhcpMessageType::Offer as u8]);
        out.extend_from_slice(&[OPTION_SERVER_ID, 4]);
        out.extend_from_slice(&server);
        out.extend_from_slice(&[OPTION_LEASE_TIME, 4]);
        out.extend_from_slice(&lease.to_be_bytes());
        out.extend_from_slice(&[OPTION_SUBNET_MASK, 4, 255, 255, 255, 0]);
        out.extend_from_slice(&[OPTION_ROUTER, 4, 192, 168, 1, 1]);
        out.extend_from_slice(&[OPTION_DNS, 8, 8, 8, 8, 8, 1, 1, 1, 1]);
        out.push(OPTION_END);
        out
    }

    #[test]
    fn discover_has_bootp_header_cookie_and_xid() {
        let client = DhcpClient::with_mac([2, 3, 4, 5, 6, 7]);
        let buf = client.build_discover();
        assert!(buf.len() > BOOTP_FIXED_LEN + 4 + 3);
        assert_eq!(buf[0], 1);
        assert_eq!(buf[1], 1);
        assert_eq!(buf[2], 6);
        assert_eq!(&buf[4..8], &client.xid.to_be_bytes());
        assert_eq!(&buf[28..34], &[2, 3, 4, 5, 6, 7]);
        assert_eq!(&buf[BOOTP_FIXED_LEN..BOOTP_FIXED_LEN + 4], &MAGIC_COOKIE);
        assert_eq!(
            &buf[BOOTP_FIXED_LEN + 4..BOOTP_FIXED_LEN + 7],
            &[OPTION_MESSAGE_TYPE, 1, DhcpMessageType::Discover as u8]
        );
        assert_eq!(buf.last(), Some(&OPTION_END));
    }

    #[test]
    fn request_contains_offered_address_and_server() {
        let mut client = DhcpClient::new();
        client.state = DhcpState::Selecting;
        client.apply_offer(
            Ipv4Addr::new([192, 168, 1, 1]),
            Ipv4Addr::new([192, 168, 1, 100]),
        );
        assert_eq!(client.offered_addr, Ipv4Addr::new([192, 168, 1, 100]));
        let request = client.build_request().unwrap();
        assert!(request.windows(6).any(|w| w == [50, 4, 192, 168, 1, 100]));
        assert!(request.windows(6).any(|w| w == [54, 4, 192, 168, 1, 1]));
    }

    #[test]
    fn offer_then_ack_runs_full_state_machine() {
        let mut client = DhcpClient::new();
        client.state = DhcpState::Selecting;
        let raw = offer(client.xid, [192, 168, 1, 1], [192, 168, 1, 100], 3600);
        let reply = client.handle_reply(&raw).unwrap();
        assert_eq!(reply.message_type, DhcpMessageType::Offer);
        assert_eq!(client.state, DhcpState::Requesting);
        assert_eq!(client.offered_addr, Ipv4Addr::new([192, 168, 1, 100]));
        assert_eq!(client.server, Some(Ipv4Addr::new([192, 168, 1, 1])));

        let mut ack = raw;
        ack[BOOTP_FIXED_LEN + 6] = DhcpMessageType::Ack as u8;
        client.handle_reply(&ack).unwrap();
        assert_eq!(client.state, DhcpState::Bound);
        assert_eq!(client.addr, Ipv4Addr::new([192, 168, 1, 100]));
        assert_eq!(client.lease_secs, 3600);
        assert_eq!(client.router, Some(Ipv4Addr::new([192, 168, 1, 1])));
        assert_eq!(client.dns_servers.len(), 2);
    }

    #[test]
    fn wrong_xid_and_malformed_cookie_are_rejected() {
        let mut client = DhcpClient::new();
        client.state = DhcpState::Selecting;
        let raw = offer(client.xid.wrapping_add(1), [1, 1, 1, 1], [10, 0, 0, 2], 60);
        assert!(client.handle_reply(&raw).is_none());
        let mut bad = offer(client.xid, [1, 1, 1, 1], [10, 0, 0, 2], 60);
        bad[BOOTP_FIXED_LEN] = 0;
        assert!(parse_reply(&bad).is_none());
    }
}
