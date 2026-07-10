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
