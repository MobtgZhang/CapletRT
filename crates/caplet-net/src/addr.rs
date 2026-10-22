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

    /// Construct from a slice. Returns `None` unless the slice is
    /// exactly four bytes long.
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 4 {
            return None;
        }
        Some(Self([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Convert to host-order `u32` (the first octet is the most
    /// significant byte).
    pub const fn to_u32(self) -> u32 {
        u32::from_be_bytes(self.0)
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

    /// Whether this is the unspecified address.
    pub const fn is_unspecified(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    /// Whether this is the limited broadcast address.
    pub const fn is_broadcast(&self) -> bool {
        self.0[0] == 255 && self.0[1] == 255 && self.0[2] == 255 && self.0[3] == 255
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

    /// Whether this is the wildcard / unbound port (zero).
    pub const fn is_unspecified(self) -> bool {
        self.0 == 0
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

    #[test]
    fn ipv4_helpers() {
        let a = Ipv4Addr::from_slice(&[192, 168, 1, 2]).unwrap();
        assert_eq!(a.to_u32(), 0xc0a8_0102);
        assert!(!a.is_unspecified());
        assert!(Ipv4Addr::BROADCAST.is_broadcast());
        assert!(Ipv4Addr::from_slice(&[1, 2, 3]).is_none());
    }
}
