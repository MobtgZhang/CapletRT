//! Network interface abstraction.
//!
//! Every concrete network device (a STM32 ETH MAC, a QEMU
//! loopback, a userspace TAP) implements [`NetworkInterface`].
//! The stack calls these methods; the implementation does the
//! MMIO / DMA / IRQ handling.

use alloc::vec::Vec;
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::addr::{Ipv4Addr, MacAddr};

/// Maximum transmission unit — Ethernet standard.
pub const MTU: usize = 1500;
/// Maximum packet size including Ethernet header (14 bytes).
pub const MAX_PACKET: usize = MTU + 14;

/// Error returned by a network interface operation.
///
/// Interfaces report *why* a packet could not be handled rather than a
/// unit error, so drivers can translate hardware status into stack-level
/// behaviour (retry, drop, or propagate to the syscall caller).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    /// The transmit ring is full; the caller may retry later.
    BufferExhausted,
    /// The device reported a hardware/DMA failure.
    Hardware,
    /// The interface is not ready (link down, not configured).
    NotReady,
}

/// A packet waiting to be transmitted.
#[derive(Debug)]
pub struct TxPacket {
    /// Destination MAC.
    pub dst: MacAddr,
    /// Payload (already includes EtherType-prefixed L3 data).
    pub payload: Vec<u8>,
    /// Explicit EtherType. When `None`, it is inferred from the first
    /// nibble of the payload (IPv4 or IPv6). ARP callers must set this.
    pub ethertype: Option<u16>,
}

impl TxPacket {
    /// Serialise this packet as an Ethernet II frame. `src` is the
    /// source MAC of the transmitting interface; the EtherType is
    /// selected from the first nibble of the L3 payload.
    pub fn to_ethernet_frame(&self, src: MacAddr) -> Vec<u8> {
        let ethertype = self
            .ethertype
            .unwrap_or_else(|| match self.payload.first() {
                Some(v) if v >> 4 == 4 => crate::eth::ethertype::IPV4,
                Some(v) if v >> 4 == 6 => 0x86dd,
                _ => crate::eth::ethertype::IPV4,
            });
        crate::eth::build_frame(self.dst, src, ethertype, &self.payload)
    }
}

/// A packet just received.
#[derive(Debug)]
pub struct RxPacket {
    /// Destination MAC.
    pub dst: MacAddr,
    /// Source MAC.
    pub src: MacAddr,
    /// EtherType (big-endian `u16`).
    pub ethertype: u16,
    /// Payload starting at the L3 header.
    pub payload: Vec<u8>,
}

/// Capability-protected handle to a network interface.
pub trait NetworkInterface {
    /// MAC address of this interface.
    fn mac(&self) -> MacAddr;

    /// IPv4 address of this interface. May be `UNSPECIFIED`
    /// if DHCP has not yet completed.
    fn ipv4(&self) -> Ipv4Addr;

    /// Set the IPv4 address.
    fn set_ipv4(&self, addr: Ipv4Addr);

    /// Transmit one packet. Returns [`NetError::BufferExhausted`] when the
    /// transmit ring is full and [`NetError::Hardware`] on device failure.
    fn transmit(&self, pkt: TxPacket) -> Result<(), NetError>;

    /// Poll for one received packet, if any. Returns
    /// `Ok(None)` if no packet is currently waiting.
    fn poll_rx(&self) -> Result<Option<RxPacket>, NetError>;
}

/// A loopback network interface used by tests and by
/// `industrial-demo` when no real PHY is present.
///
/// Transmitted packets are stored in an internal ring so the
/// test harness can verify the stack emitted the right bytes.
pub struct LoopbackInterface {
    /// MAC. Defaults to `02:00:00:00:00:01`.
    pub mac: MacAddr,
    /// IPv4 address (interior-mutable so `set_ipv4` can be
    /// called through `&self`).
    pub ipv4: AtomicU32,
    /// TX queue.
    pub tx_queue: SpinLock<Vec<TxPacket>>,
    /// RX queue (packets injected from outside for test
    /// purposes).
    pub rx_queue: SpinLock<Vec<RxPacket>>,
}

impl LoopbackInterface {
    /// Construct a new loopback interface.
    pub fn new() -> Self {
        Self {
            mac: MacAddr::new([0x02, 0, 0, 0, 0, 1]),
            ipv4: AtomicU32::new(0),
            tx_queue: SpinLock::new(Vec::new()),
            rx_queue: SpinLock::new(Vec::new()),
        }
    }

    /// Inject a packet from "outside" (e.g. a test harness
    /// simulating an incoming UDP datagram).
    pub fn inject_rx(&self, pkt: RxPacket) {
        self.rx_queue.lock().push(pkt);
    }

    /// Drain the TX queue (returns the captured packets).
    pub fn drain_tx(&self) -> Vec<TxPacket> {
        let mut q = self.tx_queue.lock();
        // Drain via std::mem::take to swap in a fresh Vec.
        let taken: Vec<TxPacket> = core::mem::take(&mut *q);
        taken
    }
}

impl Default for LoopbackInterface {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkInterface for LoopbackInterface {
    fn mac(&self) -> MacAddr {
        self.mac
    }
    fn ipv4(&self) -> Ipv4Addr {
        Ipv4Addr::new(self.ipv4.load(Ordering::Acquire).to_be_bytes())
    }
    fn set_ipv4(&self, addr: Ipv4Addr) {
        self.ipv4.store(addr.to_u32(), Ordering::Release);
    }
    fn transmit(&self, pkt: TxPacket) -> Result<(), NetError> {
        self.tx_queue.lock().push(pkt);
        Ok(())
    }
    fn poll_rx(&self) -> Result<Option<RxPacket>, NetError> {
        Ok(self.rx_queue.lock().pop())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_round_trip() {
        let lb = LoopbackInterface::new();
        let rx = RxPacket {
            dst: lb.mac(),
            src: MacAddr::new([0x02, 0, 0, 0, 0, 2]),
            ethertype: 0x0800,
            payload: alloc::vec![1, 2, 3, 4],
        };
        lb.inject_rx(rx);
        let got = lb.poll_rx().unwrap().unwrap();
        assert_eq!(got.payload, alloc::vec![1, 2, 3, 4]);
    }

    #[test]
    fn loopback_tx_capture() {
        let lb = LoopbackInterface::new();
        lb.transmit(TxPacket {
            dst: MacAddr::BROADCAST,
            payload: alloc::vec![0xde, 0xad],
            ethertype: None,
        })
        .unwrap();
        let captured = lb.drain_tx();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].payload, alloc::vec![0xde, 0xad]);
    }

    #[test]
    fn tx_packet_serialises_ethernet_header() {
        let lb = LoopbackInterface::new();
        let pkt = TxPacket {
            dst: MacAddr::BROADCAST,
            payload: alloc::vec![0x45, 0, 0, 20],
            ethertype: None,
        };
        let frame = pkt.to_ethernet_frame(lb.mac());
        let parsed = crate::eth::parse_eth(&frame).unwrap();
        assert_eq!(parsed.dst, MacAddr::BROADCAST);
        assert_eq!(parsed.src, lb.mac());
        assert_eq!(parsed.ethertype, crate::eth::ethertype::IPV4);
        assert_eq!(parsed.payload, &[0x45, 0, 0, 20]);
    }
}
