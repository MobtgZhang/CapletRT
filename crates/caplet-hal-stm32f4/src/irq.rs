//! Cortex-M NVIC interrupt controller with registered callbacks.

use alloc::boxed::Box;
use caplet_hal::interrupt::{InterruptPriority, IrqHandler};
use caplet_hal::HalError;
use core::cell::UnsafeCell;
use cortex_m::interrupt::InterruptNumber;

/// Number of external Cortex-M interrupt lines.
pub const EXTERNAL_IRQ_COUNT: usize = 240;

type Handler = Box<dyn FnMut() + Send + 'static>;

struct Slot {
    handler: Option<Handler>,
    epoch: u32,
}

const EMPTY_SLOT: Slot = Slot {
    handler: None,
    epoch: 0,
};

struct HandlerTable(UnsafeCell<[Slot; EXTERNAL_IRQ_COUNT]>);

// SAFETY: every access to the table happens inside a critical section, and a
// callback is removed from the table before it is invoked so it is never
// aliased by two interrupt contexts.
unsafe impl Sync for HandlerTable {}

static HANDLERS: HandlerTable = HandlerTable(UnsafeCell::new([EMPTY_SLOT; EXTERNAL_IRQ_COUNT]));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IrqNumber(u16);

// SAFETY: IrqNumber is only constructed from a validated external interrupt
// number in the range 0..240.
unsafe impl InterruptNumber for IrqNumber {
    fn number(self) -> u16 {
        self.0
    }
}

/// NVIC-backed implementation of [`IrqHandler`] for STM32F4.
#[derive(Debug, Default)]
pub struct IrqController;

impl IrqController {
    /// Create an interrupt controller handle.
    pub const fn new() -> Self {
        Self
    }

    fn valid(irq: u8) -> Result<IrqNumber, HalError> {
        if usize::from(irq) < EXTERNAL_IRQ_COUNT {
            Ok(IrqNumber(u16::from(irq)))
        } else {
            Err(HalError::InvalidArgument)
        }
    }

    /// Invoke the callback registered for `irq` from the board's vector entry.
    ///
    /// Returns `true` when a handler was invoked. The callback is temporarily
    /// removed from the table so it can call `register`/`unregister` without
    /// deadlocking; it is reinstalled only when the slot was not changed while
    /// the callback ran.
    pub fn dispatch(irq: u8) -> bool {
        let index = usize::from(irq);
        if index >= EXTERNAL_IRQ_COUNT {
            return false;
        }
        let (maybe_handler, epoch) = critical_section::with(|_| unsafe {
            let slot = &mut (*HANDLERS.0.get())[index];
            (slot.handler.take(), slot.epoch)
        });
        let Some(mut handler) = maybe_handler else {
            return false;
        };
        handler();
        critical_section::with(|_| unsafe {
            let slot = &mut (*HANDLERS.0.get())[index];
            if slot.handler.is_none() && slot.epoch == epoch {
                slot.handler = Some(handler);
            }
        });
        true
    }
}

impl IrqHandler for IrqController {
    type Error = HalError;

    fn register<F>(&mut self, irq: u8, handler: F) -> Result<(), Self::Error>
    where
        F: FnMut() + Send + 'static,
    {
        let index = usize::from(Self::valid(irq)?.0);
        let new_handler: Handler = Box::new(handler);
        let old = critical_section::with(|_| unsafe {
            let slot = &mut (*HANDLERS.0.get())[index];
            slot.epoch = slot.epoch.wrapping_add(1);
            slot.handler.replace(new_handler)
        });
        drop(old);
        Ok(())
    }

    fn enable(&mut self, irq: u8) -> Result<(), Self::Error> {
        let number = Self::valid(irq)?;
        // SAFETY: the number was validated against the Cortex-M external IRQ
        // range and the caller owns the board's interrupt policy.
        unsafe { cortex_m::peripheral::NVIC::unmask(number) };
        Ok(())
    }

    fn disable(&mut self, irq: u8) -> Result<(), Self::Error> {
        cortex_m::peripheral::NVIC::mask(Self::valid(irq)?);
        Ok(())
    }

    fn set_priority(&mut self, irq: u8, priority: InterruptPriority) -> Result<(), Self::Error> {
        let number = Self::valid(irq)?;
        let mut nvic = unsafe { cortex_m::Peripherals::steal().NVIC };
        // SAFETY: the interrupt number is valid and the 4-bit priority is
        // placed in the architectural upper nibble.
        unsafe { nvic.set_priority(number, priority.as_u8() << 4) };
        Ok(())
    }

    fn unregister(&mut self, irq: u8) -> Result<(), Self::Error> {
        let number = Self::valid(irq)?;
        cortex_m::peripheral::NVIC::mask(number);
        let old = critical_section::with(|_| unsafe {
            let slot = &mut (*HANDLERS.0.get())[usize::from(number.0)];
            slot.epoch = slot.epoch.wrapping_add(1);
            slot.handler.take()
        });
        drop(old);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_interrupt_numbers_outside_nvic_range() {
        assert!(IrqController::valid(0).is_ok());
        assert!(IrqController::valid(239).is_ok());
        assert_eq!(IrqController::valid(240), Err(HalError::InvalidArgument));
        assert!(!IrqController::dispatch(240));
    }
}
