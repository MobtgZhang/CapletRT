//! GPIO port implementation

use crate::raw::{self, nrf52832};
use caplet_hal::gpio::{GpioMode, GpioPin, GpioPort, OutputType, PinState, PullMode, Speed};
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
        1 => raw::nrf52840::GPIO_P1,
        _ => nrf52832::GPIO_P0,
    }
}

const PIN_CNF_DIR: u32 = 1 << 0;
const PIN_CNF_INPUT: u32 = 1 << 1;
const PIN_CNF_PULL_MASK: u32 = 0b11 << 2;
const PIN_CNF_DRIVE_MASK: u32 = 0b111 << 8;

impl Port {
    /// Take a port. `port` is the port index (0 = P0, 1 = P1, …).
    ///
    /// # Safety
    ///
    /// The caller must ensure exclusive ownership of the selected GPIO port;
    /// creating two drivers for the same port aliases its MMIO registers.
    pub unsafe fn take(port: u8) -> Self {
        assert!(port <= 1, "nRF52 GPIO port must be P0 or P1");
        Self {
            base: port_base(port) as *mut raw::GpioBlock,
            port,
        }
    }

    fn pin_cnf_addr(&self, pin: u8) -> usize {
        self.base as usize + core::mem::offset_of!(raw::GpioBlock, pin_cnf) + pin as usize * 4
    }

    unsafe fn update_pin_cnf(&self, pin: u8, mask: u32, value: u32) {
        let addr = self.pin_cnf_addr(pin);
        let current = raw::read32(addr);
        raw::write32(addr, (current & !mask) | (value & mask));
    }

    /// Configure a pin via [`PortMode`].
    pub fn configure(&mut self, pin: Pin, mode: PortMode) -> Result<(), HalError> {
        if pin.idx > 31 {
            return Err(HalError::InvalidArgument);
        }
        unsafe {
            match mode {
                PortMode::Output => {
                    self.update_pin_cnf(pin.idx, PIN_CNF_DIR | PIN_CNF_INPUT, PIN_CNF_DIR)
                }
                PortMode::Input => {
                    self.update_pin_cnf(pin.idx, PIN_CNF_DIR | PIN_CNF_INPUT, PIN_CNF_INPUT)
                }
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
        unsafe { ((*self.base).in_ & 0xFFFF) as u16 }
    }
    fn write(&mut self, value: u16) -> Result<(), Self::Error> {
        // Compute OUTSET/OUTCLR delta.
        let cur = unsafe { (*self.base).out } & 0xFFFF;
        let diff = (cur ^ value as u32) as u16;
        let set = diff & value;
        let clr = diff & !value;
        unsafe {
            if set != 0 {
                (*self.base).outset = set as u32;
            }
            if clr != 0 {
                (*self.base).outclr = clr as u32;
            }
        }
        Ok(())
    }
    fn set(&mut self, mask: u16) -> Result<(), Self::Error> {
        unsafe { (*self.base).outset = mask as u32 }
        Ok(())
    }
    fn clear(&mut self, mask: u16) -> Result<(), Self::Error> {
        unsafe { (*self.base).outclr = mask as u32 }
        Ok(())
    }
    fn toggle(&mut self, mask: u16) -> Result<(), Self::Error> {
        let cur = GpioPort::read(self) & mask;
        let set = mask & !cur;
        let clr = mask & cur;
        unsafe {
            if set != 0 {
                (*self.base).outset = set as u32;
            }
            if clr != 0 {
                (*self.base).outclr = clr as u32;
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
            GpioMode::Output => self.port.configure(
                Pin {
                    idx: self.idx,
                    port: self.port.port,
                },
                PortMode::Output,
            ),
            GpioMode::Alternate(_) | GpioMode::Analog => {
                // Peripherals select pins through PSEL, so only disconnect the
                // GPIO input buffer and release the output driver here.
                unsafe {
                    self.port
                        .update_pin_cnf(self.idx, PIN_CNF_DIR | PIN_CNF_INPUT, 0);
                }
                Ok(())
            }
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
        let bits = match pull {
            PullMode::None => 0u32,
            PullMode::Down => 1u32 << 2,
            PullMode::Up => 3u32 << 2,
        };
        unsafe {
            self.port.update_pin_cnf(self.idx, PIN_CNF_PULL_MASK, bits);
        }
        Ok(())
    }

    fn set_output_type(&mut self, output_type: OutputType) -> Result<(), Self::Error> {
        match output_type {
            OutputType::PushPull => Ok(()),
            OutputType::OpenDrain => Err(HalError::NotSupported),
        }
    }

    fn set_speed(&mut self, speed: Speed) -> Result<(), Self::Error> {
        let drive = match speed {
            Speed::Low | Speed::Medium => 0,
            Speed::High | Speed::VeryHigh => 1,
        };
        unsafe {
            self.port
                .update_pin_cnf(self.idx, PIN_CNF_DRIVE_MASK, drive << 8);
        }
        Ok(())
    }
}
