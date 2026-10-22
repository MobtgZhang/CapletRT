//! Single-core critical-section implementation for the
//! Nucleo-F446ZE board (and any other Cortex-M target without
//! the TrustZone-M `csingleton` token).
//!
//! Disables interrupts on acquire, restores the previous
//! PRIMASK on release. Safe under the assumption that no
//! interrupt will attempt to recursively acquire the same
//! critical section (true for CapletRT's kernel lock).

struct SingleCoreCriticalSection;

critical_section::set_impl!(SingleCoreCriticalSection);

unsafe impl critical_section::Impl for SingleCoreCriticalSection {
    unsafe fn acquire() -> critical_section::RawRestoreState {
        let primask: u32;
        core::arch::asm!(
            "mrs {0}, PRIMASK",
            out(reg) primask,
            options(pure, nomem, preserves_flags)
        );
        core::arch::asm!("cpsid i", options(nomem, preserves_flags));
        primask
    }

    unsafe fn release(restore: critical_section::RawRestoreState) {
        if restore & 1 == 0 {
            core::arch::asm!("cpsie i", options(nomem, preserves_flags));
        }
    }
}
