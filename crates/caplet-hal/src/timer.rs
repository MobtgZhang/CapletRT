//! Timer trait definitions

/// Timer configuration
#[derive(Debug, Clone, Copy)]
pub struct TimerConfig {
    /// Period in microseconds
    pub period_us: u32,
    /// Auto-reload enabled
    pub auto_reload: bool,
    /// Whether the timer fires once or repeats.
    pub mode: TimerRunMode,
}

impl TimerConfig {
    /// Build a one-shot configuration.
    pub const fn oneshot(period_us: u32) -> Self {
        Self {
            period_us,
            auto_reload: false,
            mode: TimerRunMode::OneShot,
        }
    }

    /// Build a periodic configuration.
    pub const fn periodic(period_us: u32) -> Self {
        Self {
            period_us,
            auto_reload: true,
            mode: TimerRunMode::Periodic,
        }
    }
}

/// One-shot versus periodic timer operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerRunMode {
    /// Stop after the first compare event.
    OneShot,
    /// Reload automatically after each compare event.
    Periodic,
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

    /// Change the run mode without changing the programmed period.
    fn set_run_mode(&mut self, mode: TimerRunMode) -> Result<(), Self::Error>;

    /// Program an absolute deadline in timer ticks and start in one-shot
    /// mode. `deadline_ticks` uses the same counter domain as
    /// [`Timer::now_ticks`].
    fn set_deadline_ticks(&mut self, deadline_ticks: u32) -> Result<(), Self::Error>;

    /// Read the current free-running counter in ticks.
    fn now_ticks(&self) -> u32 {
        self.count()
    }

    /// Configure and start a one-shot timer.
    fn oneshot(&mut self, period_us: u32) -> Result<(), Self::Error> {
        self.configure(TimerConfig::oneshot(period_us))?;
        self.start()
    }

    /// Configure and start a periodic timer.
    fn periodic(&mut self, period_us: u32) -> Result<(), Self::Error> {
        self.configure(TimerConfig::periodic(period_us))?;
        self.start()
    }
}
