//! DHCP client — minimal DISCOVER / OFFER / REQUEST / ACK
//! state machine. A real implementation is out of scope; this
//! module models the surface the rest of the stack uses.

use crate::addr::Ipv4Addr;

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

/// DHCP client state holder.
#[derive(Debug)]
pub struct DhcpClient {
    /// State.
    pub state: DhcpState,
    /// Current bound address.
    pub addr: Ipv4Addr,
    /// Transaction id.
    pub xid: u32,
    /// Server address.
    pub server: Option<Ipv4Addr>,
    /// Lease time in seconds.
    pub lease_secs: u32,
}

impl Default for DhcpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DhcpClient {
    /// Construct a new DHCP client in the Init state.
    pub fn new() -> Self {
        Self {
            state: DhcpState::Init,
            addr: Ipv4Addr::UNSPECIFIED,
            xid: 0xdeadbeef,
            server: None,
            lease_secs: 0,
        }
    }

    /// Build a DHCP DISCOVER message payload (the part after
    /// the IP/UDP headers). The buffer is filled with a
    /// minimal magic-cookie + option 53 (DHCP Message Type =
    /// Discover) + option 255 (End).
    pub fn build_discover(&self) -> alloc::vec::Vec<u8> {
        let mut out = alloc::vec::Vec::with_capacity(20);
        out.extend_from_slice(&[99, 130, 83, 99]); // magic cookie
        out.push(53);
        out.push(1);
        out.push(DhcpMessageType::Discover as u8);
        out.push(55); // parameter request list
        out.push(1);
        out.push(3); // requested: router
        out.push(255); // end
        out
    }

    /// Apply a DHCP OFFER: move from Selecting to Requesting
    /// and remember the server.
    pub fn apply_offer(&mut self, server: Ipv4Addr, _offered: Ipv4Addr) {
        if self.state == DhcpState::Selecting {
            self.server = Some(server);
            self.state = DhcpState::Requesting;
        }
    }

    /// Apply a DHCP ACK: become Bound.
    pub fn apply_ack(&mut self, addr: Ipv4Addr, lease: u32) {
        if matches!(self.state, DhcpState::Requesting | DhcpState::Renewing) {
            self.addr = addr;
            self.lease_secs = lease;
            self.state = DhcpState::Bound;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_message_format() {
        let c = DhcpClient::new();
        let buf = c.build_discover();
        assert_eq!(&buf[..4], &[99, 130, 83, 99]);
        assert_eq!(buf[6], DhcpMessageType::Discover as u8);
        assert_eq!(buf.last(), Some(&255));
    }

    #[test]
    fn state_machine_off_then_ack() {
        let mut c = DhcpClient::new();
        c.state = DhcpState::Selecting;
        c.apply_offer(Ipv4Addr::new([192, 168, 1, 1]), Ipv4Addr::new([192, 168, 1, 100]));
        assert_eq!(c.state, DhcpState::Requesting);
        c.apply_ack(Ipv4Addr::new([192, 168, 1, 100]), 3600);
        assert_eq!(c.state, DhcpState::Bound);
        assert_eq!(c.lease_secs, 3600);
    }
}