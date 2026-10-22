//! Wake source enumeration

use crate::waker::CapletWaker;
use alloc::sync::Arc;

/// Source of a wake-up event
#[derive(Debug, Clone)]
pub enum WakeSource {
    /// Timer-based wakeup
    Timer(u64),
    /// Wakeup from another task
    Task(Arc<CapletWaker>),
    /// Wakeup from an interrupt
    Interrupt(u32),
    /// Wakeup from a channel message
    Channel(usize),
    /// Wakeup from a sync primitive
    SyncPrimitive(&'static str),
    /// Manual wakeup
    Manual,
    /// Initial wakeup (first time)
    Initial,
}

impl WakeSource {
    /// Get the type name for debugging
    pub fn type_name(&self) -> &'static str {
        match self {
            WakeSource::Timer(_) => "Timer",
            WakeSource::Task(_) => "Task",
            WakeSource::Interrupt(_) => "Interrupt",
            WakeSource::Channel(_) => "Channel",
            WakeSource::SyncPrimitive(_) => "SyncPrimitive",
            WakeSource::Manual => "Manual",
            WakeSource::Initial => "Initial",
        }
    }
}
