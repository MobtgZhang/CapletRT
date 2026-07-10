//! Timer trait definitions

/// Timer configuration
#[derive(Debug, Clone, Copy)]
pub struct TimerConfig {
    /// Period in microseconds
    pub period_us: u32,
    /// Auto-reload enabled
    pub auto_reload: bool,
}

/// Timer channel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerChannel {
    /// Channel 1
    Ch1,
    /// Channel 2
    Ch2,
    /// Channel 3
    Ch3,
    /// Channel 4
    Ch4,
}

/// Timer capture/compare mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerMode {
    /// Output compare
    OutputCompare,
    /// PWM mode
    Pwm,
    /// Input capture
    InputCapture,
    /// One pulse mode
    OnePulse,
}

/// Timer trait
pub trait Timer {
    /// Error type
    type Error;

    /// Configure the timer
    fn configure(&mut self, config: TimerConfig) -> Result<(), Self::Error>;

    /// Start the timer
    fn start(&mut self) -> Result<(), Self::Error>;

    /// Stop the timer
    fn stop(&mut self) -> Result<(), Self::Error>;

    /// Reset the counter
    fn reset(&mut self) -> Result<(), Self::Error>;

    /// Get current count
    fn count(&self) -> u32;

    /// Set period
    fn set_period(&mut self, period_us: u32) -> Result<(), Self::Error>;

    /// Enable an interrupt
    fn enable_interrupt(&mut self) -> Result<(), Self::Error>;

    /// Disable an interrupt
    fn disable_interrupt(&mut self) -> Result<(), Self::Error>;

    /// Clear interrupt flag
    fn clear_interrupt(&mut self) -> Result<(), Self::Error>;

    /// Check if interrupt is pending
    fn is_interrupt_pending(&self) -> bool;
}
