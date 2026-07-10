//! Nucleo-F446ZE board crate
//!
//! Boots the Cortex-M4F at 16 MHz (HSI), installs a semihosting
//! log writer, then prints the CapletRT boot banner and blinks
//! the green LED (PB0) at 1 Hz.

#![no_std]
#![no_main]

use caplet_cortex_m::panic as _panic;
use caplet_hal_stm32f4::prelude::*;
use caplet_log::{info, install_writer, semihosting::SEMIHOSTING_INSTANCE};
use cortex_m_rt::entry;
use embedded_alloc::Heap;

mod critical_section_impl;

#[global_allocator]
static HEAP: Heap = Heap::empty();

/// Heap region — top 16K of RAM, just below the kernel stack.
const HEAP_START: usize = 0x2001_C000;
const HEAP_SIZE: usize = 16 * 1024;

/// Clock frequency after RCC configuration. We use HSI = 16 MHz.
const HSI_HZ: u32 = 16_000_000;

/// Set up clocks, peripherals, and the log writer.
fn board_init() {
    // Initialize the global heap first.
    unsafe { HEAP.init(HEAP_START, HEAP_SIZE) };

    // Install the log writer first so we can see what happens.
    install_writer(Some(&SEMIHOSTING_INSTANCE));

    // Configure clocks.
    let mut rcc = unsafe { Rcc::take() };
    rcc.hsi_16mhz();
    rcc.enable_gpioa();
    rcc.enable_gpiob();
    rcc.enable_usart2();

    // Configure the on-board LED (PB0).
    let mut gpiob = unsafe { Port::take(1) };
    let mut led = gpiob.pin(0);
    led.set_mode(GpioMode::Output).ok();
    led.set_state(PinState::Low).ok();
}

#[entry]
fn main() -> ! {
    board_init();
    info!("boot", "Nucleo-F446ZE boot OK");

    // Toggle the LED forever.
    let mut gpiob = unsafe { Port::take(1) };
    let mut led = gpiob.pin(0);
    let mut on = false;
    loop {
        on = !on;
        let _ = led.set_state(if on { PinState::High } else { PinState::Low });
        // Naive busy-wait delay — replaced by the kernel timer
        // in M2.5+.
        for _ in 0..1_000_000 {
            core::hint::spin_loop();
        }
    }
}