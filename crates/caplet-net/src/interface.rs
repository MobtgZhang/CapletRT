//! Network interface abstraction.
//!
//! Every concrete network device (a STM32 ETH MAC, a QEMU
//! loopback, a userspace TAP) implements [`NetworkInterface`].
//! The stack calls these methods; the implementation does the
//! MMIO / DMA / IRQ handling.

use alloc::vec::Vec;
use caplet_sync::SpinLock;

use crate::addr::{Ipv4Addr, MacAddr};

/// Maximum transmission unit — Ethernet standard.
pub const MTU: usize = 1500;
/// Maximum packet size including Ethernet header (14 bytes).
pub const MAX_PACKET: usize = MTU + 14;

/// A packet waiting to be transmitted.
#[derive(Debug)]
pub struct TxPacket {
    /// Destination MAC.
    pub dst: MacAddr,
    /// Payload (already includes EtherType-prefixed L3 data).
    pub payload: Vec<u8>,
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

    /// Transmit one packet. Returns `Err(())` on buffer
    /// exhaustion or hardware error.
    fn transmit(&self, pkt: TxPacket) -> Result<(), ()>;

    /// Poll for one received packet, if any. Returns
    /// `Ok(None)` if no packet is currently waiting.
    fn poll_rx(&self) -> Result<Option<RxPacket>, ()>;
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
    pub ipv4: core::cell::Cell<Ipv4Addr>,
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
            ipv4: core::cell::Cell::new(Ipv4Addr::UNSPECIFIED),
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
        self.ipv4.get()
    }
    fn set_ipv4(&self, addr: Ipv4Addr) {
        self.ipv4.set(addr);
    }    fn transmit(&self, pkt: TxPacket) -> Result<(), ()> {
        self.tx_queue.lock().push(pkt);
        Ok(())
    }
    fn poll_rx(&self) -> Result<Option<RxPacket>, ()> {
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
        })
        .unwrap();
        let captured = lb.drain_tx();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].payload, alloc::vec![0xde, 0xad]);
    }
}