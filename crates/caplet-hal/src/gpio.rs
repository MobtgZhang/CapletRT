//! GPIO trait definitions

/// GPIO pin mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpioMode {
    /// Input mode
    Input,
    /// Output mode
    Output,
    /// Alternate function
    Alternate(u8),
    /// Analog mode
    Analog,
}

/// Pin state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinState {
    /// Pin is low
    Low,
    /// Pin is high
    High,
}

impl PinState {
    /// Get boolean value
    pub fn as_bool(&self) -> bool {
        matches!(self, PinState::High)
    }

    /// Create from boolean
    pub fn from_bool(value: bool) -> Self {
        if value {
            PinState::High
        } else {
            PinState::Low
        }
    }
}

impl From<PinState> for bool {
    fn from(state: PinState) -> Self {
        state.as_bool()
    }
}

impl From<bool> for PinState {
    fn from(value: bool) -> Self {
        PinState::from_bool(value)
    }
}

/// GPIO pull mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullMode {
    /// No pull resistor
    None,
    /// Pull up resistor
    Up,
    /// Pull down resistor
    Down,
}

/// Output type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputType {
    /// Push-pull output
    PushPull,
    /// Open-drain output
    OpenDrain,
}

/// Speed configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    /// Low speed
    Low,
    /// Medium speed
    Medium,
    /// High speed
    High,
    /// Very high speed
    VeryHigh,
}

/// GPIO pin trait
pub trait GpioPin {
    /// Error type
    type Error;

    /// Get the pin number
    fn pin(&self) -> u8;

    /// Set the pin mode
    fn set_mode(&mut self, mode: GpioMode) -> Result<(), Self::Error>;

    /// Set the output state
    fn set_state(&mut self, state: PinState) -> Result<(), Self::Error>;

    /// Get the input state
    fn get_state(&self) -> Result<PinState, Self::Error>;

    /// Toggle the output state
    fn toggle(&mut self) -> Result<(), Self::Error>;

    /// Set pull mode
    fn set_pull(&mut self, pull: PullMode) -> Result<(), Self::Error>;

    /// Set output type
    fn set_output_type(&mut self, output_type: OutputType) -> Result<(), Self::Error>;

    /// Set speed
    fn set_speed(&mut self, speed: Speed) -> Result<(), Self::Error>;

    /// Consume the pin and configure it as a high-impedance input.
    fn into_input(mut self) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        self.set_mode(GpioMode::Input)?;
        Ok(self)
    }

    /// Consume the pin and configure it as a push-pull output.
    fn into_output(mut self) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        self.set_mode(GpioMode::Output)?;
        self.set_output_type(OutputType::PushPull)?;
        Ok(self)
    }

    /// Consume the pin and configure it as an open-drain output.
    fn into_open_drain(mut self) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        self.set_mode(GpioMode::Output)?;
        self.set_output_type(OutputType::OpenDrain)?;
        Ok(self)
    }

    /// Attach an interrupt callback to this pin.
    ///
    /// The pin is configured as an input. The concrete controller is
    /// responsible for registering the callback with the NVIC/GIC/PLIC; this
    /// HAL method only carries the pin and callback together so the board
    /// cannot accidentally hand the callback a pin in output mode.
    fn into_irq<F>(mut self, handler: F) -> Result<IrqPin<Self, F>, Self::Error>
    where
        Self: Sized,
    {
        self.set_mode(GpioMode::Input)?;
        Ok(IrqPin { pin: self, handler })
    }
}

/// A GPIO pin paired with an interrupt callback.
pub struct IrqPin<P, F> {
    pin: P,
    handler: F,
}

impl<P: GpioPin, F> IrqPin<P, F> {
    /// Return the pin number.
    pub fn pin(&self) -> u8 {
        self.pin.pin()
    }

    /// Borrow the callback without invoking it.
    pub fn handler(&self) -> &F {
        &self.handler
    }

    /// Mutably borrow the callback.
    pub fn handler_mut(&mut self) -> &mut F {
        &mut self.handler
    }

    /// Split the pair back into its pin and callback.
    pub fn release(self) -> (P, F) {
        (self.pin, self.handler)
    }
}

impl<P: GpioPin, F> IrqPin<P, F>
where
    F: FnMut() + Send + 'static,
{
    /// Register the callback with a platform interrupt controller.
    ///
    /// The callback is moved into the controller; the GPIO pin is returned so
    /// the caller can continue to read it from the interrupt handler (or drop
    /// it if the controller owns the line for the rest of the boot).
    pub fn register_with<I>(self, irq: u8, controller: &mut I) -> Result<P, I::Error>
    where
        I: crate::interrupt::IrqHandler,
    {
        controller.register(irq, self.handler)?;
        Ok(self.pin)
    }
}

/// GPIO port trait
pub trait GpioPort {
    /// Error type
    type Error;

    /// Get port number
    fn port(&self) -> u8;

    /// Read all pins in the port
    fn read(&self) -> u16;

    /// Write to all pins in the port
    fn write(&mut self, value: u16) -> Result<(), Self::Error>;

    /// Set pins high
    fn set(&mut self, mask: u16) -> Result<(), Self::Error>;

    /// Set pins low
    fn clear(&mut self, mask: u16) -> Result<(), Self::Error>;

    /// Toggle pins
    fn toggle(&mut self, mask: u16) -> Result<(), Self::Error>;
}
