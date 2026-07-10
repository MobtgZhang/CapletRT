//! CapletRT Synchronization Primitives
//!
//! This crate provides synchronization primitives for CapletRT.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

extern crate alloc;

pub mod spin;
pub mod mutex;
pub mod semaphore;
pub mod channel;
pub mod sync_channel;
pub mod event;
pub mod ticket_lock;

pub use spin::SpinLock;
pub use mutex::Mutex;
pub use semaphore::Semaphore;
pub use channel::{Channel, Sender, Receiver, channel};
pub use sync_channel::{SyncChannel, SyncSender, SyncReceiver, sync_channel};
pub use event::Event;
pub use ticket_lock::{TicketLock, TicketGuard};

/// Architecture-specific "relax the CPU" hint. On x86 this is `pause`,
/// on ARM this is a `yield` instruction. Used inside busy-wait loops
/// and the idle task to be friendly to the system bus.
pub fn cpu_relax() {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("yield", options(nomem, nostack, preserves_flags));
    }
    #[cfg(target_arch = "arm")]
    unsafe {
        core::arch::asm!("yield", options(nomem, nostack, preserves_flags));
    }
    #[cfg(not(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "arm"
    )))]
    {
        core::hint::spin_loop();
    }
}