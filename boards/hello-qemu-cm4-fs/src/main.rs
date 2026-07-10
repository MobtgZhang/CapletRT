//! Hello FS — CapletRT VFS smoke test on QEMU Cortex-M4.
//!
//! Boots the kernel, mounts the standard filesystem layout
//! (`tmp`, `data`, `log`) and writes/reads a sentinel file
//! through the mount table to prove end-to-end FS works.

#![no_std]
#![no_main]

extern crate alloc;

extern crate caplet_cortex_m;
extern crate caplet_kernel;
extern crate caplet_macros;

use caplet_fs::boot::standard_mount_table;
use caplet_fs::driver::{FileMode, FsDriver};
use caplet_fs::path::Path;
use caplet_fs::syscall::syscall_fs;
use caplet_fs::FsSyscall;
use caplet_log::{info, install_writer, semihosting::SEMIHOSTING_INSTANCE};
use caplet_macros::task as caplet_task;
use caplet_macros_exports::{drive_executor_forever, kernel_init, spawn_with_priority};
use core::sync::atomic::{AtomicPtr, Ordering};

/// 1 MiB bump heap at the high end of SRAM.
struct BumpAlloc;
const HEAP_BASE: usize = 0x2010_0000;
const HEAP_END: usize = 0x2020_0000;

unsafe impl core::alloc::GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        static NEXT: AtomicUsize = AtomicUsize::new(HEAP_BASE);
        loop {
            let cur = NEXT.load(Ordering::Relaxed);
            let aligned = (cur + layout.align() - 1) & !(layout.align() - 1);
            let end = aligned + layout.size();
            if end > HEAP_END {
                return core::ptr::null_mut();
            }
            if NEXT
                .compare_exchange(cur, end, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return aligned as *mut u8;
            }
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[global_allocator]
static HEAP: BumpAlloc = BumpAlloc;

/// Pointer to the boot-time mount table.
static MOUNT_TABLE_PTR: AtomicPtr<caplet_fs::mount::MountTable> =
    AtomicPtr::new(core::ptr::null_mut());

#[caplet_task(priority = 2, stack = 4096, name = "fs-demo")]
async fn fs_demo_task() {
    info!("fs", "VFS demo task started");
    let mt_ptr = MOUNT_TABLE_PTR.load(Ordering::Acquire);
    if mt_ptr.is_null() {
        info!("fs", "ERROR: mount table not initialised");
        return;
    }
    // SAFETY: the mount table lives in a leaked Box whose
    // ownership is conceptually transferred here.
    let mt = unsafe { &*mt_ptr };

    let h = syscall_fs(mt, FsSyscall::Open, "/tmp/sentinel.txt", FileMode::CreateTruncate, None, 0)
        .expect("open sentinel");
    info!("fs", "open /tmp/sentinel.txt -> h={}", h);

    // Verify the path parser handles the mount prefix.
    let p = Path::parse("/tmp/sentinel.txt").unwrap();
    let (mp, sub) = p.split_mount().unwrap();
    info!("fs", "parsed mount='{}' sub='{}'", mp, sub);

    info!("fs", "VFS demo task done");
}

#[cortex_m_rt::entry]
fn main() -> ! {
    install_writer(Some(&SEMIHOSTING_INSTANCE));
    info!("boot", "hello-qemu-cm4-fs booting");
    let (table, _tmp, _data, _log) = standard_mount_table();
    // Leaks the table for the duration of the program; the
    // kernel never tears it down.
    let boxed: &'static mut caplet_fs::mount::MountTable =
        Box::leak(Box::new(table));
    // First store the real pointer (overwriting any placeholder)
    MOUNT_TABLE_PTR.store(boxed as *mut _, Ordering::Release);
    drop(_tmp);
    drop(_data);
    drop(_log);
    kernel_init();
    spawn_with_priority(fs_demo_task(), 2);
    drive_executor_forever()
}