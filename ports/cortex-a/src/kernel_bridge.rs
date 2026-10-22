//! Bridge between the Cortex-A port and the capability kernel.
//!
//! The board owns the reset vector and early platform setup. It then
//! calls [`kernel_main_with_console`], which builds two real task
//! contexts, enqueues them in the kernel scheduler and performs
//! cooperative A32 context switches through [`crate::context`].

use crate::context::build_initial_stack;
use crate::pl011_uart::{Pl011Uart, PL011_BASE};
use caplet_kernel::scheduler::ScheduledTask;
use caplet_kernel::{cap_init_with_config, yield_current_task, KernelConfig, Priority};
use core::sync::atomic::{AtomicUsize, Ordering};

/// Stack size for each demonstration task.
const DEMO_STACK_SIZE: usize = 16 * 1024;
/// Number of cooperative rounds each task performs.
const DEMO_ROUNDS: usize = 4;

/// Callback used by the shared scheduler tasks to write the platform
/// console.
#[derive(Clone, Copy)]
pub struct KernelConsole {
    /// Write one UTF-8 string to the platform console.
    pub write: fn(&str),
    /// Human-readable platform name used in the success marker.
    pub platform: &'static str,
}

impl KernelConsole {
    /// Construct a console description.
    pub const fn new(write: fn(&str), platform: &'static str) -> Self {
        Self { write, platform }
    }
}

#[repr(align(16))]
struct TaskStack([u8; DEMO_STACK_SIZE]);

static mut TASK_A_STACK: TaskStack = TaskStack([0; DEMO_STACK_SIZE]);
static mut TASK_B_STACK: TaskStack = TaskStack([0; DEMO_STACK_SIZE]);

static CONSOLE_WRITE: AtomicUsize = AtomicUsize::new(0);
static PLATFORM_PTR: AtomicUsize = AtomicUsize::new(0);
static PLATFORM_LEN: AtomicUsize = AtomicUsize::new(0);

fn write(text: &str) {
    let address = CONSOLE_WRITE.load(Ordering::Acquire);
    if address == 0 {
        return;
    }
    // SAFETY: the address was published from a `fn(&str)` at boot and
    // remains valid for the lifetime of the image.
    let callback: fn(&str) = unsafe { core::mem::transmute(address) };
    callback(text);
}

fn write_dec(mut value: usize) {
    let mut buffer = [0u8; 20];
    let mut index = buffer.len();
    loop {
        index -= 1;
        buffer[index] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    // SAFETY: the slice contains only ASCII digits written above.
    let text = unsafe { core::str::from_utf8_unchecked(&buffer[index..]) };
    write(text);
}

fn platform_name() -> &'static str {
    let pointer = PLATFORM_PTR.load(Ordering::Acquire) as *const u8;
    let length = PLATFORM_LEN.load(Ordering::Acquire);
    if pointer.is_null() || length == 0 {
        return "Cortex-A";
    }
    // SAFETY: both values were published from a `'static` string literal.
    unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(pointer, length)) }
}

extern "C" fn demo_task_a(arg: u32) -> ! {
    for round in 0..DEMO_ROUNDS {
        write("kernel: task A arg=");
        write_dec(arg as usize);
        write(" round=");
        write_dec(round);
        write("\n");
        yield_current_task();
    }
    write("kernel: task A done\n");
    yield_current_task();
    park()
}

extern "C" fn demo_task_b(arg: u32) -> ! {
    for round in 0..DEMO_ROUNDS {
        write("kernel: task B arg=");
        write_dec(arg as usize);
        write(" round=");
        write_dec(round);
        write("\n");
        if round == 0 {
            write("CapletRT ");
            write(platform_name());
            write(" kernel scheduler OK\n");
        }
        yield_current_task();
    }
    write("kernel: task B done\n");
    park()
}

fn park() -> ! {
    loop {
        #[cfg(target_arch = "arm")]
        // SAFETY: WFI is a low-power hint with no memory side effects.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
        #[cfg(not(target_arch = "arm"))]
        core::hint::spin_loop();
    }
}

unsafe extern "C" fn kernel_switch(prev: *mut usize, next: *mut usize) {
    // SAFETY: forwarded from `Kernel::start_with_context_switch`.
    unsafe { crate::context::__caplet_switch(prev, next as *const usize) }
}

fn make_task(id: u32, priority: u8, entry: u32, arg: u32, stack_top: u32) -> ScheduledTask {
    let context = build_initial_stack(stack_top as *mut u32, entry, arg) as usize;
    let mut task = ScheduledTask::new(id, Priority::new(priority), entry as usize, context)
        .with_stack_size(DEMO_STACK_SIZE as u32);
    task.context = context;
    task
}

/// Install a platform console when the board does not run the shared
/// scheduler. Kept public so the panic path and bring-up code can use
/// the same writer.
pub fn install_console(console: KernelConsole) {
    CONSOLE_WRITE.store(console.write as usize, Ordering::Release);
    PLATFORM_PTR.store(console.platform.as_ptr() as usize, Ordering::Release);
    PLATFORM_LEN.store(console.platform.len(), Ordering::Release);
}

/// Start the capability kernel and run two cooperative tasks forever.
pub fn kernel_main_with_console(console: KernelConsole) -> ! {
    install_console(console);
    write("kernel: initialising capability kernel\n");
    let kernel = cap_init_with_config(KernelConfig::cortex_a9());
    write("kernel: capability kernel ready\n");

    // SAFETY: the stacks are static and are only handed to these two
    // task contexts during single-threaded boot.
    let top_a = unsafe { core::ptr::addr_of_mut!(TASK_A_STACK.0) } as usize + DEMO_STACK_SIZE;
    let top_b = unsafe { core::ptr::addr_of_mut!(TASK_B_STACK.0) } as usize + DEMO_STACK_SIZE;
    let task_a = make_task(1, 10, demo_task_a as *const () as u32, 0xA1, top_a as u32);
    let task_b = make_task(2, 20, demo_task_b as *const () as u32, 0xB2, top_b as u32);
    write("kernel: contexts built\n");
    kernel.add_task(task_a);
    kernel.add_task(task_b);
    write("kernel: tasks enqueued\n");
    write("kernel: starting cooperative scheduler\n");
    kernel.start_with_context_switch(kernel_switch)
}

fn pl011_write(text: &str) {
    Pl011Uart::new(PL011_BASE).puts(text);
}

/// Default Cortex-A entry point used by [`crate::boot::cortex_a_boot`].
///
/// Boards with their own reset stub call [`kernel_main_with_console`]
/// directly and may use a different UART (for example the A20's
/// NS16550).  This entry keeps the architecture port's generic boot
/// path functional on the vexpress PL011.
pub extern "C" fn rust_main() -> ! {
    let mut uart = Pl011Uart::new(PL011_BASE);
    uart.init();
    uart.install();
    kernel_main_with_console(KernelConsole::new(pl011_write, "Cortex-A"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_descriptor_is_copy() {
        fn sink(_: &str) {}
        let console = KernelConsole::new(sink, "test");
        let copy = console;
        assert_eq!(console.platform, copy.platform);
    }

    #[test]
    fn decimal_writer_handles_zero() {
        static mut OUTPUT: [u8; 24] = [0; 24];
        static mut LEN: usize = 0;
        fn sink(text: &str) {
            // SAFETY: test-only single-threaded sink.
            unsafe {
                let len = LEN;
                let bytes = text.as_bytes();
                OUTPUT[len..len + bytes.len()].copy_from_slice(bytes);
                LEN += bytes.len();
            }
        }
        install_console(KernelConsole::new(sink, "test"));
        write_dec(0);
        // SAFETY: see above.
        unsafe {
            assert_eq!(&OUTPUT[..LEN], b"0");
            LEN = 0;
        }
    }
}
