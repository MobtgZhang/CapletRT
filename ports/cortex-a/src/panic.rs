//! Panic handling for Cortex-A.
//!
//! docv1.md §10.5 mandates fail-stop: halt + log on panic. In the
//! QEMU smoke test the log goes to the PL011 UART.

#[cfg(all(target_os = "none", feature = "panic-handler"))]
use crate::pl011_uart::Pl011Uart;

/// Shim that lets the panic handler wait for an interrupt forever.
#[cfg(all(target_os = "none", feature = "panic-handler"))]
mod cortex_a_arch {
    #[cfg(target_os = "none")]
    pub unsafe fn wfi() {
        #[cfg(target_arch = "arm")]
        {
            core::arch::asm!("wfi", options(nostack, preserves_flags));
        }
        #[cfg(not(target_arch = "arm"))]
        {
            core::hint::spin_loop();
        }
    }
}

#[cfg(all(target_os = "none", not(test), feature = "panic-handler"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Try to grab the UART and dump a message. If the UART is not
    // initialised, fall back to a busy loop.
    let mut uart = Pl011Uart::take();
    if let Some(u) = uart.as_mut() {
        u.puts("PANIC: ");
        if let Some(s) = info.message().as_str() {
            u.puts(s);
        } else {
            u.puts("(non-string panic)");
        }
        u.puts("\n");
    }
    loop {
        unsafe {
            cortex_a_arch::wfi();
        }
        core::hint::spin_loop();
    }
}
