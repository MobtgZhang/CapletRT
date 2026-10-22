//! Nucleo-F446ZE board crate
//!
//! Boots the Cortex-M4F at 16 MHz (HSI), installs a semihosting
//! log writer, starts the capability kernel and drives two real async
//! tasks.  The LED task toggles PB0 through GPIOB_BSRR while the
//! monitor task yields cooperatively and reports scheduler progress.

#![no_std]
#![no_main]

extern crate alloc;
extern crate caplet_cortex_m;
extern crate caplet_kernel;
extern crate caplet_macros;

use caplet::task as caplet_task;
use caplet_hal_stm32f4::prelude::*;
use caplet_log::{info, install_writer, semihosting::SEMIHOSTING_INSTANCE};
use caplet_macros_exports::{drive_executor_forever, kernel_init, TaskTable};
use core::sync::atomic::{AtomicU32, Ordering};
use cortex_m_rt::entry;
use embedded_alloc::Heap;

mod critical_section_impl;

#[global_allocator]
static HEAP: Heap = Heap::empty();

/// Rust heap below the kernel boot heap (`0x2001_0000..0x2001_4000`).
/// The 8 KiB reset stack occupies the top of the 128 KiB SRAM.
const HEAP_START: usize = 0x2000_1000;
const HEAP_SIZE: usize = 60 * 1024;

/// GPIOB base + BSRR offset on STM32F446ZE.
const GPIOB_BSRR: *mut u32 = 0x4002_0418 as *mut u32;
const LED_SET: u32 = 1 << 0;
const LED_RESET: u32 = 1 << 16;

static LED_ROUNDS: AtomicU32 = AtomicU32::new(0);
static MONITOR_ROUNDS: AtomicU32 = AtomicU32::new(0);

async fn yield_now() {
    YieldOnce(false).await
}

struct YieldOnce(bool);

impl core::future::Future for YieldOnce {
    type Output = ();

    fn poll(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<()> {
        if self.0 {
            core::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    }
}

/// Drive the user LED for six rounds, yielding between every update.
#[caplet_task(priority = 2, stack = 1024, name = "led")]
async fn led_task() {
    info!("nucleo", "task led started");
    for round in 0..6u32 {
        let value = if round % 2 == 0 { LED_SET } else { LED_RESET };
        // SAFETY: BSRR is the write-only set/reset register of GPIOB, whose
        // clock is enabled by board_init before the executor starts.
        unsafe { core::ptr::write_volatile(GPIOB_BSRR, value) };
        LED_ROUNDS.store(round + 1, Ordering::Release);
        info!(
            "nucleo",
            "task led round={} state={}",
            round,
            if round % 2 == 0 { "on" } else { "off" }
        );
        yield_now().await;
    }
    info!("nucleo", "task led done");
}

/// Observe the LED task across scheduler cycles and report completion.
#[caplet_task(priority = 1, stack = 1024, name = "monitor")]
async fn monitor_task() {
    info!("nucleo", "task monitor started");
    for round in 0..12u32 {
        MONITOR_ROUNDS.store(round + 1, Ordering::Release);
        if round == 0 || round == 11 {
            info!(
                "nucleo",
                "task monitor round={} led_rounds={}",
                round,
                LED_ROUNDS.load(Ordering::Acquire)
            );
        }
        yield_now().await;
    }
    info!("nucleo", "task monitor done");
    info!("nucleo", "Nucleo-F446ZE kernel scheduler OK");
}

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

    kernel_init();

    let mut tasks = TaskTable::<4>::new();
    let mut explicit = 0;
    if __caplet_register_task_led_task(&mut tasks) {
        explicit += 1;
    }
    if __caplet_register_task_monitor_task(&mut tasks) {
        explicit += 1;
    }
    let (launched, skipped) = tasks.start_all();
    info!(
        "task_table",
        "linked=0 explicit={} launched={} skipped={}", explicit, launched, skipped
    );

    drive_executor_forever()
}
