//! UARTE driver (no-PAC, MMIO only).

use crate::raw::{self, nrf52832};
use caplet_hal::uart::{Parity, StopBits, Uart, UartConfig};
use caplet_hal::HalError;

const UARTE_ENABLE: u32 = 8;
const PIN_DISCONNECTED: u32 = 0xFFFF_FFFF;
const ERROR_OVERRUN: u32 = 1 << 0;
const ERROR_PARITY: u32 = 1 << 1;
const ERROR_FRAMING: u32 = 1 << 2;

/// Pin assignment for a UARTE instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UartPins {
    /// TXD pin index (P0 or P1 depending on the selected port).
    pub txd: u8,
    /// RXD pin index.
    pub rxd: u8,
    /// Optional CTS pin. Must be supplied together with `rts`.
    pub cts: Option<u8>,
    /// Optional RTS pin. Must be supplied together with `cts`.
    pub rts: Option<u8>,
}

/// UARTE instance.
pub struct Uarte {
    base: *mut raw::UarteBlock,
    pins: Option<UartPins>,
    tx_active: bool,
}

unsafe impl Send for Uarte {}

impl Uarte {
    /// Take UARTE0.
    ///
    /// # Safety
    ///
    /// The caller must ensure UARTE0 is not already owned by another driver
    /// and that the configured pins do not conflict with other peripherals.
    pub unsafe fn uarte0() -> Self {
        Self {
            base: nrf52832::UARTE0 as *mut raw::UarteBlock,
            pins: None,
            tx_active: false,
        }
    }

    /// Take UARTE1.
    ///
    /// # Safety
    ///
    /// The caller must ensure UARTE1 is not already owned by another driver
    /// and that the configured pins do not conflict with other peripherals.
    pub unsafe fn uarte1() -> Self {
        Self {
            base: nrf52832::UARTE1 as *mut raw::UarteBlock,
            pins: None,
            tx_active: false,
        }
    }

    /// Assign TXD/RXD and optional hardware-flow-control pins.
    ///
    /// Both `cts` and `rts` must be present to enable hardware flow control;
    /// supplying only one is rejected rather than silently ignored.
    pub fn set_pins(
        &mut self,
        txd: u8,
        rxd: u8,
        cts: Option<u8>,
        rts: Option<u8>,
    ) -> Result<(), HalError> {
        if txd > 31
            || rxd > 31
            || cts.is_some_and(|pin| pin > 31)
            || rts.is_some_and(|pin| pin > 31)
        {
            return Err(HalError::InvalidArgument);
        }
        if cts.is_some() != rts.is_some() {
            return Err(HalError::InvalidArgument);
        }
        self.pins = Some(UartPins { txd, rxd, cts, rts });
        unsafe { self.apply_pins() };
        Ok(())
    }

    unsafe fn apply_pins(&self) {
        let pins = self.pins;
        let block = &mut *self.base;
        block.psel_txd = pins.map_or(PIN_DISCONNECTED, |value| u32::from(value.txd));
        block.psel_rxd = pins.map_or(PIN_DISCONNECTED, |value| u32::from(value.rxd));
        block.psel_cts = pins
            .and_then(|value| value.cts)
            .map_or(PIN_DISCONNECTED, u32::from);
        block.psel_rts = pins
            .and_then(|value| value.rts)
            .map_or(PIN_DISCONNECTED, u32::from);
    }

    fn baud_selector(baud_rate: u32) -> Option<u32> {
        Some(match baud_rate {
            1_200 => 0x0004_F000,
            2_400 => 0x0009_D000,
            4_800 => 0x0013_B000,
            9_600 => 0x0027_5000,
            14_400 => 0x003A_F000,
            19_200 => 0x004E_A000,
            28_800 => 0x0075_6000,
            31_250 => 0x0080_0000,
            38_400 => 0x009D_0000,
            57_600 => 0x00EB_0000,
            76_800 => 0x013A_9000,
            115_200 => 0x01D6_0000,
            230_400 => 0x03B0_0000,
            250_000 => 0x0400_0000,
            460_800 => 0x0740_0000,
            921_600 => 0x0F00_0000,
            1_000_000 => 0x1000_0000,
            _ => return None,
        })
    }

    unsafe fn check_error(&self) -> Result<(), HalError> {
        let block = &mut *self.base;
        if block.events_error == 0 {
            return Ok(());
        }
        let source = block.errorsrc;
        // ERRORSRC is write-one-to-clear: write back the captured flags.
        core::ptr::write_volatile(core::ptr::addr_of_mut!(block.errorsrc), source);
        block.events_error = 0;
        if source & ERROR_OVERRUN != 0 {
            Err(HalError::Overrun)
        } else if source & ERROR_PARITY != 0 {
            Err(HalError::Parity)
        } else if source & ERROR_FRAMING != 0 {
            Err(HalError::Framing)
        } else {
            Err(HalError::HardwareError)
        }
    }

    unsafe fn wait_event(&self, event: *mut u32) -> Result<(), HalError> {
        loop {
            self.check_error()?;
            if core::ptr::read_volatile(event) != 0 {
                core::ptr::write_volatile(event, 0);
                return Ok(());
            }
            core::hint::spin_loop();
        }
    }
}

impl Uart for Uarte {
    type Error = HalError;

    fn configure(&mut self, config: UartConfig) -> Result<(), Self::Error> {
        if config.data_bits != 8 || config.stop_bits != StopBits::One {
            return Err(HalError::NotSupported);
        }
        let baud = Self::baud_selector(config.baud_rate).ok_or(HalError::NotSupported)?;
        let mut uarte_config = 0u32;
        if config.parity != Parity::None {
            uarte_config |= 1 << 1;
            if config.parity == Parity::Odd {
                uarte_config |= 1 << 2;
            }
        }
        if self
            .pins
            .is_some_and(|pins| pins.cts.is_some() && pins.rts.is_some())
        {
            uarte_config |= 1;
        }

        unsafe {
            let block = &mut *self.base;
            block.enable = 0;
            block.events_rxdrdy = 0;
            block.events_endrx = 0;
            block.events_txdrdy = 0;
            block.events_endtx = 0;
            block.events_error = 0;
            block.errorsrc = u32::MAX;
            self.apply_pins();
            block.baudrate = baud;
            block.config = uarte_config;
            block.enable = UARTE_ENABLE;
        }
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, Self::Error> {
        if data.is_empty() {
            return Ok(0);
        }
        let length = u32::try_from(data.len()).map_err(|_| HalError::InvalidArgument)?;
        unsafe {
            let block = &mut *self.base;
            if block.enable != UARTE_ENABLE {
                return Err(HalError::NotReady);
            }
            block.events_txdrdy = 0;
            block.events_endtx = 0;
            block.events_error = 0;
            block.txd_ptr = data.as_ptr() as u32;
            block.txd_maxcnt = length;
            self.tx_active = true;
            block.tasks_starttx = 1;
            let result = self.wait_event(core::ptr::addr_of_mut!(block.events_endtx));
            self.tx_active = false;
            result?;
        }
        Ok(data.len())
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let length = u32::try_from(buffer.len()).map_err(|_| HalError::InvalidArgument)?;
        unsafe {
            let block = &mut *self.base;
            if block.enable != UARTE_ENABLE {
                return Err(HalError::NotReady);
            }
            block.events_rxdrdy = 0;
            block.events_endrx = 0;
            block.events_error = 0;
            block.rxd_ptr = buffer.as_mut_ptr() as u32;
            block.rxd_maxcnt = length;
            block.tasks_startrx = 1;
            let result = self.wait_event(core::ptr::addr_of_mut!(block.events_endrx));
            block.tasks_stoprx = 1;
            result?;
            Ok((block.rxd_amount as usize).min(buffer.len()))
        }
    }

    fn available(&self) -> bool {
        unsafe { (*self.base).events_rxdrdy != 0 }
    }

    fn tx_empty(&self) -> bool {
        !self.tx_active
    }
}

impl core::fmt::Write for Uarte {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = self.write_all(s.as_bytes());
        Ok(())
    }
}
