//! GPIO port + pin implementation

use crate::raw::{self, f446};
use caplet_hal::gpio::{
    GpioMode, GpioPin, GpioPort, OutputType, PinState, PullMode, Speed,
};
use caplet_hal::HalError;

/// GPIO pin identifier (port + index 0..15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    /// 0..=15
    pub idx: u8,
    /// 0 = A, 1 = B, 2 = C, 3 = D, …
    pub port: u8,
}

/// Port mode marker — used in board init code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortMode {
    /// Input
    Input,
    /// General-purpose output
    Output,
    /// Alternate function
    Alternate(u8),
    /// Analog
    Analog,
}

/// GPIO port driver. Cheap to construct; can be cloned by hand.
pub struct Port {
    base: *mut raw::GpioBlock,
    port: u8,
}

// SAFETY: GPIO registers are written under our control; the board
// init code typically disables interrupts during configuration.
unsafe impl Send for Port {}

fn port_base(port: u8) -> usize {
    match port {
        0 => f446::GPIOA,
        1 => f446::GPIOB,
        2 => f446::GPIOC,
        3 => f446::GPIOD,
        4 => f446::GPIOE,
        5 => f446::GPIOF,
        6 => f446::GPIOG,
        7 => f446::GPIOH,
        _ => f446::GPIOA,
    }
}

impl Port {
    /// Take a port. `port` is the port index (0 = A, 1 = B, …).
    pub unsafe fn take(port: u8) -> Self {
        Self {
            base: port_base(port) as *mut raw::GpioBlock,
            port,
        }
    }

    /// Configure a pin via the [`PortMode`] enum (more ergonomic
    /// than the trait-level [`GpioPin::set_mode`]).
    pub fn configure(&mut self, pin: Pin, mode: PortMode) -> Result<(), HalError> {
        let p = pin.idx as usize;
        if p > 15 {
            return Err(HalError::InvalidArgument);
        }
        unsafe {
            let moder = raw::read32(self.base as usize + 0x00);
            let new_mode = match mode {
                PortMode::Input => 0b00,
                PortMode::Output => 0b01,
                PortMode::Alternate(_) => 0b10,
                PortMode::Analog => 0b11,
            };
            let moder = (moder & !(0b11 << (p * 2))) | (new_mode << (p * 2));
            raw::write32(self.base as usize + 0x00, moder);

            if let PortMode::Alternate(af) = mode {
                let af: u32 = (af & 0xF) as u32;
                if p < 8 {
                    let afrl = raw::read32(self.base as usize + 0x20);
                    let afrl = (afrl & !(0xFu32 << (p * 4))) | (af << (p * 4));
                    raw::write32(self.base as usize + 0x20, afrl);
                } else {
                    let p2 = p - 8;
                    let afrh = raw::read32(self.base as usize + 0x24);
                    let afrh = (afrh & !(0xFu32 << (p2 * 4))) | (af << (p2 * 4));
                    raw::write32(self.base as usize + 0x24, afrh);
                }
            }
        }
        Ok(())
    }

    /// Return a [`Pin`] handle that delegates back through this
    /// port. The handle holds a `&mut Port`, so you cannot mix
    /// `Pin` and `Port` access in the same scope.
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
        unsafe { (raw::read32(self.base as usize + 0x10) & 0xFFFF) as u16 }
    }
    fn write(&mut self, value: u16) -> Result<(), Self::Error> {
        unsafe {
            raw::write32(self.base as usize + 0x14, value as u32);
        }
        Ok(())
    }
    fn set(&mut self, mask: u16) -> Result<(), Self::Error> {
        unsafe {
            raw::write32(self.base as usize + 0x18, mask as u32);
        }
        Ok(())
    }
    fn clear(&mut self, mask: u16) -> Result<(), Self::Error> {
        unsafe {
            raw::write32(self.base as usize + 0x18, (mask as u32) << 16);
        }
        Ok(())
    }
    fn toggle(&mut self, mask: u16) -> Result<(), Self::Error> {
        let odr = self.read();
        self.write(odr ^ mask)
    }
}

/// A `Pin` borrowing its parent [`Port`].
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
        let port_mode = match mode {
            GpioMode::Input => PortMode::Input,
            GpioMode::Output => PortMode::Output,
            GpioMode::Alternate(af) => PortMode::Alternate(af),
            GpioMode::Analog => PortMode::Analog,
        };
        self.port.configure(
            Pin {
                idx: self.idx,
                port: self.port.port,
            },
            port_mode,
        )
    }

    fn set_state(&mut self, state: PinState) -> Result<(), Self::Error> {
        match state {
            PinState::High => self
                .port
                .set(1 << self.idx as u16)
                .map_err(|_| HalError::HardwareError),
            PinState::Low => self
                .port
                .clear(1 << self.idx as u16)
                .map_err(|_| HalError::HardwareError),
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
        let p = self.idx as usize;
        let pull_bits = match pull {
            PullMode::None => 0b00,
            PullMode::Up => 0b01,
            PullMode::Down => 0b10,
        };
        unsafe {
            let pupdr = raw::read32(self.port.base as usize + 0x0C);
            let pupdr = (pupdr & !(0b11 << (p * 2))) | (pull_bits << (p * 2));
            raw::write32(self.port.base as usize + 0x0C, pupdr);
        }
        Ok(())
    }

    fn set_output_type(&mut self, output_type: OutputType) -> Result<(), Self::Error> {
        let p = self.idx as usize;
        let bit = match output_type {
            OutputType::PushPull => 0,
            OutputType::OpenDrain => 1,
        };
        unsafe {
            let otyper = raw::read32(self.port.base as usize + 0x04);
            let otyper = (otyper & !(1 << p)) | (bit << p);
            raw::write32(self.port.base as usize + 0x04, otyper);
        }
        Ok(())
    }

    fn set_speed(&mut self, speed: Speed) -> Result<(), Self::Error> {
        let p = self.idx as usize;
        let bits = match speed {
            Speed::Low => 0b00,
            Speed::Medium => 0b01,
            Speed::High => 0b10,
            Speed::VeryHigh => 0b11,
        };
        unsafe {
            let ospeedr = raw::read32(self.port.base as usize + 0x08);
            let ospeedr = (ospeedr & !(0b11 << (p * 2))) | (bits << (p * 2));
            raw::write32(self.port.base as usize + 0x08, ospeedr);
        }
        Ok(())
    }
}