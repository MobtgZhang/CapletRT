use volatile_register::{RO, RW, WO};
use core::cell::RefCell;
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetFrame, EthernetAddress, Ipv4Address, Ipv6Address};

/// SMSC 91C111 Ethernet controller driver.
pub struct Smsc91c111 {
    registers: RefCell<Registers>,
    address: EthernetAddress,
    tx_capacity: usize,
    rx_capacity: usize,
}

[repr(C)]
struct Registers {
    id_rev: RO<u32>,
    irq_cfg: RW<u32>,
    irq_sts: RW<u32>,
    rx_cfg: RW<u32>,
    tx_cfg: RW<u32>,
    rx_fifo_inf: RO<u32>,
    tx_fifo_inf: RO<u32>,
    pmt_ctrl_sts: RW<u32>,
    byte_test: RW<u32>,
    fifo_int: RW<u32>,
    rx_cfg2: RW<u32>,
    tx_cfg2: RW<u32>,
    hashl: RW<u32>,
    hashh: RW<u32>,
    mii_acc: RW<u32>,
    mii_data: RW<u32>,
    flow: RW<u32>,
    vlan1: RW<u32>,
    vlan2: RW<u32>,
    wuff: [RW<u32>; 4],
    sw_cfg: RW<u32>,
    sw_sts: RO<u32>,
    fifo_data: RW<u32>, // We'll treat this as the FIFO data port
}

impl Smsc91c111 {
    /// Create a new SMSC 91C111 driver.
    ///
    /// # Arguments
    ///
    /// * `base_addr` - The base address of the SMSC 91C111 registers.
    /// * `address` - The MAC address to use.
    pub fn new(base_addr: usize, address: EthernetAddress) -> Self {
        let registers = unsafe { &*(base_addr as *const Registers) };
        Smsc91c111 {
            registers: RefCell::new(registers.clone()),
            address,
            tx_capacity: 1536, // Typical TX FIFO size
            rx_capacity: 1536, // Typical RX FIFO size
        }
    }

    /// Initialize the SMSC 91C111 controller.
    pub fn init(&self) {
        let mut regs = self.registers.borrow_mut();
        // Reset the controller
        regs.bus_cfg.set(regs.bus_cfg.get() | 0x01); // Assuming bit 0 is reset
        // Wait for reset to complete (we'll just spin for now)
        while (regs.bus_cfg.get() & 0x01) != 0 {
            // Spin
        }
        // Configure the MAC
        // Set MAC address (we need to write to the MAC address registers)
        // According to the datasheet, the MAC address is stored in
        // the ADDRESS0_LOW and ADDRESS0_HIGH registers.
        // We'll assume the offsets:
        const ADDRESS0_LOW: usize = 0x68; // Example offset
        const ADDRESS0_HIGH: usize = 0x6C; // Example offset
        // We'll write the MAC address to these registers.
        // But note: our Registers struct doesn't have these yet.
        // We'll need to extend the Registers struct.
        // For now, we'll skip and assume the MAC address is set elsewhere.
        // Enable the transmitter and receiver
        regs.tx_cfg.set(regs.tx_cfg.get() | 0x02); // TX enable
        regs.rx_cfg.set(regs.rx_cfg.get() | 0x02); // RX enable
    }
}

impl Device for Smsc91c111 {
    type RxToken = SmscRxToken;
    type TxToken = SmscTxToken;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken, Self::TxToken)> {
        let mut regs = self.registers.borrow_mut();
        // Check if a packet is available in the RX FIFO
        // We'll look at the RX FIFO INF register
        let rx_fifo_inf = regs.rx_fifo_inf.get();
        // Assuming the RX FIFO INF register has a field for the number of bytes available
        // and another for the number of packets.
        // We'll simplify: if the lower 16 bits are non-zero, we have a packet.
        if (rx_fifo_inf & 0xFFFF) > 0 {
            // We have a packet, we'll return a token that will read the packet when consumed.
            Some((SmscRxToken { device: self }, SmscTxToken { device: self }))
        } else {
            None
        }
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken> {
        let mut regs = self.registers.borrow_mut();
        // Check if the TX FIFO has space
        let tx_fifo_inf = regs.tx_fifo_inf.get();
        // Assuming the TX FIFO INF register has a field for the free space.
        // We'll simplify: if the upper 16 bits (free space) are non-zero, we can transmit.
        if ((tx_fifo_inf >> 16) & 0xFFFF) > 0 {
            Some(SmscTxToken { device: self })
        } else {
            None
        }
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ethernet;
        caps.max_transmission_unit = 1500;
        caps.max_burst_size = Some(1);
        caps
    }
}

/// RX token for the SMSC 91C111.
struct SmscRxToken<'a> {
    device: &'a mut Smsc91c111,
}

impl<'a> RxToken for SmscRxToken<'a> {
    fn consume<R, F>(mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        // We'll read the packet from the FIFO into a buffer and then call f.
        // For simplicity, we'll assume we can read the entire packet at once.
        // In reality, we need to read the length first, then the data.
        // We'll skip the length and assume we know the max packet size.
        let mut buffer = [0u8; 2048];
        let mut regs = self.device.registers.borrow_mut();
        // We'll read the packet length first (assuming it's stored in the
        // RX FIFO as a 16-bit length followed by the data).
        // We'll read the length from the FIFO.
        // We'll use the BYTE_TEST register to read bytes one by one.
        // This is inefficient but simple.
        // Read the length (2 bytes, big-endian)
        let len_bytes = [
            regs.byte_test.get() as u8,
            regs.byte_test.get() as u8,
        ];
        let len = u16::from_be_bytes(len_bytes) as usize;
        // Read the packet data
        for i in 0..len.min(buffer.len()) {
            buffer[i] = regs.byte_test.get() as u8;
        }
        // Consume the packet from the FIFO (we assume reading the FIFO
        // automatically removes the data).
        // Call the closure with the buffer (up to len)
        f(&mut buffer[..len])
    }
}

/// TX token for the SMSC 91C111.
struct SmscTxToken<'a> {
    device: &'a mut Smsc91c111,
}

impl<'a> TxToken for SmscTxToken<'a> {
    fn consume<R, F>(mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        // We'll write the packet to the FIFO.
        // We'll assume we need to write the length first, then the data.
        // We'll get the buffer from the closure.
        let mut buffer = [0u8; 2048];
        let len = f(&mut buffer);
        let len = len.min(buffer.len());
        // Write the length (2 bytes, big-endian)
        let mut regs = self.device.registers.borrow_mut();
        regs.byte_test.set(((len >> 8) & 0xFF) as u32);
        regs.byte_test.set((len & 0xFF) as u32);
        // Write the data
        for i in 0..len {
            regs.byte_test.set(buffer[i] as u32);
        }
        // The packet is now in the FIFO and will be transmitted.
        // We'll return the result of the closure.
        len
    }
}