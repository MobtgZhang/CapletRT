//! GPIO port implementation

use crate::raw::{self, nrf52832};
use caplet_hal::gpio::{
    GpioMode, GpioPin, GpioPort, OutputType, PinState, PullMode, Speed,
};
use caplet_hal::HalError;

/// Pin identifier (port + index).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    /// 0..=31
    pub idx: u8,
    /// 0 = P0, 1 = P1.
    pub port: u8,
}

/// Port mode marker — board init ergonomics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortMode {
    /// Input
    Input,
    /// Output
    Output,
}

/// GPIO port driver.
pub struct Port {
    base: *mut raw::GpioBlock,
    port: u8,
}

unsafe impl Send for Port {}

fn port_base(port: u8) -> usize {
    match port {
        0 => nrf52832::GPIO_P0,
        1 => nrf52832::GPIO_P1,
        _ => nrf52832::GPIO_P0,
    }
}

impl Port {
    /// Take a port. `port` is the port index (0 = P0, 1 = P1, …).
    pub unsafe fn take(port: u8) -> Self {
        Self {
            base: port_base(port) as *mut raw::GpioBlock,
            port,
        }
    }

    /// Configure a pin via [`PortMode`].
    pub fn configure(&mut self, pin: Pin, mode: PortMode) -> Result<(), HalError> {
        if pin.idx > 31 {
            return Err(HalError::InvalidArgument);
        }
        let bit = 1u32 << pin.idx;
        unsafe {
            match mode {
                PortMode::Output => raw::set_bits(self.base as usize + 0x04, bit),
                PortMode::Input => raw::clear_bits(self.base as usize + 0x04, bit),
            }
        }
        Ok(())
    }

    /// Get a [`PinRef`] handle.
    pub fn pin(&mut self, idx: u8) -> PinRef<'_> {
        PinRef { port: self, idx }
    }
}

impl GpioPort for Port {
    type Error = HalError;

    fn port(&self) -> u8 {
        self.port
    }
    fn read(&self) -> u16 {
        // The nRF52 GPIO `IN` register is wider than 16 bits but
        // caplet-hal only asks for u16. We truncate.
        unsafe { (raw::read32(self.base as usize + 0x28) & 0xFFFF) as u16 }
    }
    fn write(&mut self, value: u16) -> Result<(), Self::Error> {
        // Compute OUTSET/OUTCLR delta.
        let cur = unsafe { raw::read32(self.base as usize + 0x10) } & 0xFFFF;
        let diff = (cur ^ value as u32) as u16;
        let set = diff & value;
        let clr = diff & !value;
        unsafe {
            if set != 0 {
                raw::write32(self.base as usize + 0x14, set as u32);
            }
            if clr != 0 {
                raw::write32(self.base as usize + 0x18, clr as u32);
            }
        }
        Ok(())
    }
    fn set(&mut self, mask: u16) -> Result<(), Self::Error> {
        unsafe { raw::write32(self.base as usize + 0x14, mask as u32) }
        Ok(())
    }
    fn clear(&mut self, mask: u16) -> Result<(), Self::Error> {
        unsafe { raw::write32(self.base as usize + 0x18, mask as u32) }
        Ok(())
    }
    fn toggle(&mut self, mask: u16) -> Result<(), Self::Error> {
        let cur = GpioPort::read(self) & mask;
        let set = mask & !cur;
        let clr = mask & cur;
        unsafe {
            if set != 0 {
                raw::write32(self.base as usize + 0x14, set as u32);
            }
            if clr != 0 {
                raw::write32(self.base as usize + 0x18, clr as u32);
            }
        }
        Ok(())
    }
}

/// Borrowing pin handle.
pub struct PinRef<'p> {
    port: &'p mut Port,
    idx: u8,
}

impl<'p> GpioPin for PinRef<'p> {
    type Error = HalError;

    fn pin(&self) -> u8 {
        self.idx
    }

    fn set_mode(&mut self, mode: GpioMode) -> Result<(), Self::Error> {
        match mode {
            GpioMode::Input => self.port.configure(
                Pin {
                    idx: self.idx,
                    port: self.port.port,
                },
                PortMode::Input,
            ),
            GpioMode::Output | GpioMode::Alternate(_) | GpioMode::Analog => self.port.configure(
                Pin {
                    idx: self.idx,
                    port: self.port.port,
                },
                PortMode::Output,
            ),
        }
    }

    fn set_state(&mut self, state: PinState) -> Result<(), Self::Error> {
        match state {
            PinState::High => GpioPort::set(self.port, 1 << self.idx as u16),
            PinState::Low => GpioPort::clear(self.port, 1 << self.idx as u16),
        }
    }

    fn get_state(&self) -> Result<PinState, Self::Error> {
        let v = GpioPort::read(self.port);
        Ok(if v & (1 << self.idx as u16) != 0 {
            PinState::High
        } else {
            PinState::Low
        })
    }

    fn toggle(&mut self) -> Result<(), Self::Error> {
        GpioPort::toggle(self.port, 1 << self.idx as u16)
    }

    fn set_pull(&mut self, pull: PullMode) -> Result<(), Self::Error> {
        // Each pin occupies 2 bits in the PULL register. Encoding:
        // 00 = disabled, 01 = pulldown, 11 = pullup.
        let p = self.idx as usize;
        let bits = match pull {
            PullMode::None => 0u32,
            PullMode::Down => 1u32,
            PullMode::Up => 3u32,
        };
        unsafe {
            let cur = raw::read32(self.port.base as usize + 0x60);
            let cur = (cur & !(0x3u32 << (p * 2))) | (bits << (p * 2));
            raw::write32(self.port.base as usize + 0x60, cur);
        }
        Ok(())
    }

    fn set_output_type(&mut self, _output_type: OutputType) -> Result<(), Self::Error> {
        // nRF52 GPIO is push-pull only. Silently ignore.
        Ok(())
    }

    fn set_speed(&mut self, _speed: Speed) -> Result<(), Self::Error> {
        // nRF52 doesn't have configurable speed. Silently ignore.
        Ok(())
    }
}