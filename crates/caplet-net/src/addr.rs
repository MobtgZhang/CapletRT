//! Network address types.

/// IEEE 802 MAC address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MacAddr(pub [u8; 6]);

impl MacAddr {
    /// Broadcast MAC address (ff:ff:ff:ff:ff:ff).
    pub const BROADCAST: MacAddr = MacAddr([0xff; 6]);

    /// Construct from a 6-byte array.
    pub const fn new(bytes: [u8; 6]) -> Self {
        MacAddr(bytes)
    }

    /// Octets.
    pub fn octets(&self) -> &[u8; 6] {
        &self.0
    }
}

/// IPv4 address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ipv4Addr(pub [u8; 4]);

impl Ipv4Addr {
    /// The unspecified address 0.0.0.0.
    pub const UNSPECIFIED: Ipv4Addr = Ipv4Addr([0, 0, 0, 0]);
    /// The limited broadcast address 255.255.255.255.
    pub const BROADCAST: Ipv4Addr = Ipv4Addr([255, 255, 255, 255]);

    /// Construct from a 4-byte array.
    pub const fn new(bytes: [u8; 4]) -> Self {
        Ipv4Addr(bytes)
    }

    /// Octets.
    pub fn octets(&self) -> &[u8; 4] {
        &self.0
    }

    /// Whether this address is in the multicast range (224.0.0.0/4).
    pub fn is_multicast(&self) -> bool {
        self.0[0] >= 224 && self.0[0] <= 239
    }

    /// Whether this address is in the loopback range (127.0.0.0/8).
    pub fn is_loopback(&self) -> bool {
        self.0[0] == 127
    }
}

/// Transport-layer port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Port(pub u16);

impl Port {
    /// Construct from raw `u16`.
    pub const fn new(p: u16) -> Self {
        Port(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_loopback() {
        assert!(Ipv4Addr::new([127, 0, 0, 1]).is_loopback());
        assert!(!Ipv4Addr::new([128, 0, 0, 1]).is_loopback());
    }

    #[test]
    fn ipv4_multicast() {
        assert!(Ipv4Addr::new([224, 0, 0, 1]).is_multicast());
        assert!(!Ipv4Addr::new([192, 168, 1, 1]).is_multicast());
    }

    #[test]
    fn mac_broadcast() {
        assert_eq!(MacAddr::BROADCAST, MacAddr::new([0xff; 6]));
    }
}