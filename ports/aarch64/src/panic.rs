//! Panic handler for AArch64.

/// Park the CPU on panic so a debugger can inspect the failure.
#[cfg(all(target_os = "none", not(test)))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    crate::Uart::put_line("PANIC: CapletRT AArch64");
    if let Some(location) = info.location() {
        crate::Uart::put_str(location.file());
        crate::Uart::put_char(b':');
        crate::Uart::put_hex(location.line() as u64);
        crate::Uart::put_line("");
    }
    loop {
        #[cfg(target_arch = "aarch64")]
        unsafe {
            core::arch::asm!("wfe", options(nomem, nostack));
        }
        #[cfg(not(target_arch = "aarch64"))]
        core::hint::spin_loop();
    }
}
