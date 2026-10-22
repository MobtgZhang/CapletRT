//! LED trait definition

/// LED trait for simple output devices
pub trait Led {
    /// Error type
    type Error;

    /// Turn the LED on
    fn on(&mut self) -> Result<(), Self::Error>;

    /// Turn the LED off
    fn off(&mut self) -> Result<(), Self::Error>;

    /// Toggle the LED
    fn toggle(&mut self) -> Result<(), Self::Error>;

    /// Check if LED is on
    fn is_on(&self) -> bool;
}

/// Active low LED adapter
pub struct ActiveLowLed<L: Led> {
    led: L,
}

impl<L: Led> ActiveLowLed<L> {
    /// Create a new active-low LED wrapper
    pub fn new(led: L) -> Self {
        Self { led }
    }
}

impl<L: Led> Led for ActiveLowLed<L> {
    type Error = L::Error;

    fn on(&mut self) -> Result<(), Self::Error> {
        self.led.off()
    }

    fn off(&mut self) -> Result<(), Self::Error> {
        self.led.on()
    }

    fn toggle(&mut self) -> Result<(), Self::Error> {
        self.led.toggle()
    }

    fn is_on(&self) -> bool {
        !self.led.is_on()
    }
}
