//! Interrupt trait definitions

/// Interrupt trigger mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptTrigger {
    /// Rising edge
    RisingEdge,
    /// Falling edge
    FallingEdge,
    /// Both edges
    BothEdges,
    /// High level
    HighLevel,
    /// Low level
    LowLevel,
}

/// Interrupt priority
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptPriority(u8);

impl InterruptPriority {
    /// Create a new priority (0-15 for Cortex-M)
    pub fn new(priority: u8) -> Self {
        Self(priority.min(15))
    }

    /// Get the raw value
    pub fn as_u8(&self) -> u8 {
        self.0
    }

    /// Highest priority (0)
    pub fn highest() -> Self {
        Self(0)
    }

    /// Lowest priority (15)
    pub fn lowest() -> Self {
        Self(15)
    }
}

impl Default for InterruptPriority {
    fn default() -> Self {
        Self(8) // Default middle priority
    }
}

/// Interrupt trait
pub trait Interrupt {
    /// Error type
    type Error;

    /// Get the interrupt number
    fn number(&self) -> u8;

    /// Enable the interrupt
    fn enable(&mut self) -> Result<(), Self::Error>;

    /// Disable the interrupt
    fn disable(&mut self) -> Result<(), Self::Error>;

    /// Check if enabled
    fn is_enabled(&self) -> bool;

    /// Set priority
    fn set_priority(&mut self, priority: InterruptPriority) -> Result<(), Self::Error>;

    /// Get priority
    fn get_priority(&self) -> InterruptPriority;

    /// Check if pending
    fn is_pending(&self) -> bool;

    /// Clear pending flag
    fn clear_pending(&mut self) -> Result<(), Self::Error>;
}

/// A platform interrupt controller (NVIC, GIC, PLIC, ...).
///
/// `Interrupt` represents one interrupt line; this trait represents the
/// controller that owns the line. Callbacks are registered as `FnMut` values
/// and must remain valid for the lifetime of the registration, which is why
/// implementations normally store them in a static/`'static` table.
pub trait IrqHandler {
    /// Error type returned by controller operations.
    type Error;

    /// Register the handler for `irq`, replacing any previous handler.
    fn register<F>(&mut self, irq: u8, handler: F) -> Result<(), Self::Error>
    where
        F: FnMut() + Send + 'static;

    /// Enable delivery of `irq`.
    fn enable(&mut self, irq: u8) -> Result<(), Self::Error>;

    /// Disable delivery of `irq` without removing its handler.
    fn disable(&mut self, irq: u8) -> Result<(), Self::Error>;

    /// Set the priority of `irq` in the controller's priority space.
    fn set_priority(&mut self, irq: u8, priority: InterruptPriority) -> Result<(), Self::Error>;

    /// Remove a previously registered handler.
    fn unregister(&mut self, irq: u8) -> Result<(), Self::Error>;
}
