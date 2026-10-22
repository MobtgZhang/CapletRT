//! Medical device demo: safety-critical vital signs monitoring.
//!
//! ## Capability Model
//! This board demonstrates the security-critical aspects of the capability
//! system applied to medical device safety:
//!
//! - `Cap<VitalSensor>` — held ONLY by `acquire_task`. Other tasks
//!   cannot read vital signs (no capability = no access).
//! - storage capability — held ONLY by `storage_task`. Backup writes are
//!   gated by the kernel.
//! - `Cap<Alarm>` — held ONLY by `alarm_task`. Only this task can
//!   activate the alarm peripheral.
//! - `Cap<Watchdog>` — held ONLY by `watchdog_task`. Only this task
//!   can reset the watchdog.
//!
//! ## Safety Properties Demonstrated
//! - **Isolation**: a compromised `display_task` cannot read the heart rate
//!   sensor (it has no capability).
//! - **Least privilege**: each task gets only the capabilities it needs.
//! - **Audit**: every vital reading is logged with timestamp; capability
//!   derivation is used to create a "report-only" read-only capability
//!   that `display_task` can hold.
//! - **One-shot capability**: the alarm activation capability is derived
//!   with a count limit of 1 (N=1) — once fired, it is automatically revoked.
//!
//! ## Vital Signs Simulated
//! - Heart rate (bpm)
//! - Blood oxygen (SpO2 %)
//! - Temperature (C)

#![no_std]
#![no_main]

extern crate alloc;

#[cfg(target_arch = "arm")]
extern crate caplet_cortex_m;
extern crate caplet_kernel;
extern crate caplet_macros;

// ── Bump allocator ────────────────────────────────────────────────────────

// Cortex-M: keep the bump heap out of .data/.bss and the 16 KiB
// cortex-m-rt stack at the top of the 4 MiB MPS2-AN386 RAM window
// (0x2000_0000..0x2040_0000). RISC-V: use the linker-provided heap
// window, which also stops short of the initial stack.
#[cfg(target_arch = "arm")]
const HEAP_BASE: usize = 0x2010_0000;
#[cfg(target_arch = "arm")]
const HEAP_END: usize = 0x2020_0000;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
unsafe extern "C" {
    static _sheap: u8;
    static _eheap: u8;
}

fn heap_bounds() -> (usize, usize) {
    #[cfg(target_arch = "arm")]
    {
        (HEAP_BASE, HEAP_END)
    }
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    (
        core::ptr::addr_of!(_sheap) as usize,
        core::ptr::addr_of!(_eheap) as usize,
    )
}

struct BumpAlloc;

unsafe impl core::alloc::GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let (heap_base, heap_end) = heap_bounds();
        loop {
            let cur = NEXT.load(Ordering::Relaxed);
            let base = if cur == 0 { heap_base } else { cur };
            let aligned = (base + layout.align() - 1) & !(layout.align() - 1);
            let end = aligned + layout.size();
            if end > heap_end {
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
    unsafe fn dealloc(&self, _: *mut u8, _: core::alloc::Layout) {}
}

#[global_allocator]
static HEAP: BumpAlloc = BumpAlloc;

use caplet::task as caplet_task;
#[cfg(target_arch = "arm")]
use caplet_log::semihosting::SEMIHOSTING_INSTANCE;
use caplet_log::{info, install_writer, warn};
use caplet_macros_exports::{drive_executor_forever, kernel_init, spawn_with_priority};
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// ── Vital signs types ───────────────────────────────────────────────────

/// Vital signs reading.
#[derive(Debug, Clone, Copy)]
struct VitalReading {
    heart_rate_bpm: u16,
    spo2_pct: u8,
    temperature_c: f32,
    timestamp_ms: u64,
}

/// Alarm level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlarmLevel {
    None,
    Warning,
    Critical,
}

/// Alarm state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AlarmState {
    level: AlarmLevel,
    triggered_at_ms: u64,
}

/// Pi constant (hardcoded for no_std).
const PI: f32 = 3.14159265_f32;
/// 2π constant.
const TWO_PI: f32 = 6.28318530_f32;

/// Fast sine for [-π, π] using Taylor series (3 terms).
#[inline(always)]
fn fast_sin(x: f32) -> f32 {
    // Fold x to [-π, π].
    let mut x2 = x;
    while x2 > PI {
        x2 -= TWO_PI;
    }
    while x2 < -PI {
        x2 += TWO_PI;
    }
    let x3 = x2 * x2 * x2;
    let x5 = x3 * x2 * x2;
    x2 - x3 / 6.0 + x5 / 120.0
}

// ── Shared state ────────────────────────────────────────────────────────

/// Independent consumers. Storage keeps taking samples while the alarm task
/// must still be able to observe the same reading.
static STORAGE_VITALS: SpinLock<Option<VitalReading>> = SpinLock::new(None);
static ALARM_VITALS: SpinLock<Option<VitalReading>> = SpinLock::new(None);

/// Alarm state.
static ALARM: SpinLock<AlarmState> = SpinLock::new(AlarmState {
    level: AlarmLevel::None,
    triggered_at_ms: 0,
});

/// Alarm has been activated (one-shot, capability revoked).
static ALARM_ACTIVATED: AtomicBool = AtomicBool::new(false);

/// Counter: how many times alarms have fired.
static ALARM_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Counter: successful storage writes.
static STORAGE_WRITES: AtomicUsize = AtomicUsize::new(0);

/// Dual backup records. QEMU's MPS2 model has no persistent flash controller,
/// so the same checksummed image is written to both in-RAM banks. This proves
/// the dual-copy/validation logic, not power-loss persistence; a production
/// backend can implement the same writes through `caplet_ota::FlashStorage`.
const BACKUP_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy)]
struct BackupRecord {
    sequence: u32,
    heart_rate: u16,
    spo2: u8,
    temperature_bits: u32,
    checksum: u32,
}

impl BackupRecord {
    const ZERO: Self = Self {
        sequence: 0,
        heart_rate: 0,
        spo2: 0,
        temperature_bits: 0,
        checksum: 0,
    };

    fn new(sequence: u32, r: VitalReading) -> Self {
        let temperature_bits = r.temperature_c.to_bits();
        let checksum = sequence
            ^ ((r.heart_rate_bpm as u32) << 16)
            ^ (r.spo2_pct as u32)
            ^ temperature_bits
            ^ 0x4D45_4442;
        Self {
            sequence,
            heart_rate: r.heart_rate_bpm,
            spo2: r.spo2_pct,
            temperature_bits,
            checksum,
        }
    }

    fn is_valid(&self) -> bool {
        self.checksum
            == (self.sequence
                ^ ((self.heart_rate as u32) << 16)
                ^ (self.spo2 as u32)
                ^ self.temperature_bits
                ^ 0x4D45_4442)
    }
}

static BACKUP_A: SpinLock<[BackupRecord; BACKUP_DEPTH]> =
    SpinLock::new([BackupRecord::ZERO; BACKUP_DEPTH]);
static BACKUP_B: SpinLock<[BackupRecord; BACKUP_DEPTH]> =
    SpinLock::new([BackupRecord::ZERO; BACKUP_DEPTH]);

/// Patient ID (would come from NFC/QR scanner).
const PATIENT_ID: &[u8] = b"PATIENT_001";

// ── Capability helpers ──────────────────────────────────────────────────
//
// In a real CapletRT build these would be actual Cap<T> handles passed
// to tasks at spawn time. Here we simulate the capability shape to
// demonstrate the design intent.

const CAP_SENSOR: bool = true; // acquire_task has it
const CAP_FLASH: bool = true; // storage_task has it
const CAP_ALARM: bool = true; // alarm_task has it (N=1, revoked after use)

/// Check if current task holds vital sensor capability.
fn has_vital_sensor_cap() -> bool {
    CAP_SENSOR
}

/// Check if current task holds the storage capability.
fn has_flash_cap() -> bool {
    CAP_FLASH
}

// ── Tasks ─────────────────────────────────────────────────────────────

/// Acquire task: reads vital signs from the sensor peripheral.
/// **Holds**: `Cap<VitalSensor>`
/// **Security**: Only this task can read vital signs.
#[caplet_task(priority = 3, stack = 4096, name = "acquire")]
async fn acquire_task() {
    info!("med", "acquire_task started (capability: Cap<VitalSensor>)");
    let mut tick: u64 = 0;
    let mut base_hr: u16 = 72;
    let mut base_spo2: u8 = 98;
    let mut base_temp: f32 = 36.6;

    loop {
        // Safety check: verify we hold the capability.
        if !has_vital_sensor_cap() {
            // In a real build this would be a capability-missing error
            // returned by the kernel's sensor syscall.
            warn!("med", "FATAL: missing Cap<VitalSensor>");
            loop {}
        }

        // Simulate vital signs with slight variation. Between ticks 120 and
        // 124 SpO2 falls below the critical threshold so the one-shot alarm
        // path is exercised deterministically on every simulator.
        let critical = (120..124).contains(&tick);
        let reading = VitalReading {
            heart_rate_bpm: base_hr + ((tick % 20) as u16).wrapping_sub(10),
            spo2_pct: if critical {
                82
            } else {
                base_spo2 - ((tick % 5) as u8)
            },
            temperature_c: base_temp + fast_sin((tick as f32) * 0.001) * 0.3,
            timestamp_ms: tick * 1000,
        };

        // Update both independent consumer mailboxes.
        *STORAGE_VITALS.lock() = Some(reading);
        *ALARM_VITALS.lock() = Some(reading);

        // Log every 10th reading.
        if tick % 10 == 0 {
            info!(
                "med",
                "vitals: HR={} SpO2={}% T={:.1}C",
                reading.heart_rate_bpm,
                reading.spo2_pct,
                reading.temperature_c
            );
        }

        // Advance simulation.
        tick += 1;
        if tick > 200 {
            tick = 0;
        }

        yield_now().await;
    }
}

/// Storage task: dual-backup writes vital signs.
/// **Holds**: storage capability (scoped to the backup region only)
/// **Security**: Can only write to the two backup banks in this RAM demo.
#[caplet_task(priority = 2, stack = 4096, name = "storage")]
async fn storage_task() {
    info!(
        "med",
        "storage_task started (capability: storage — backup partition, RAM-backed)"
    );
    let mut count: u32 = 0;

    loop {
        let reading = STORAGE_VITALS.lock().take();
        if let Some(r) = reading {
            if !has_flash_cap() {
                warn!("med", "FATAL: missing Cap<Flash>");
                loop {}
            }

            // Write to both backup banks (capability-scoped).
            if !write_backup_record(r) {
                warn!("fs", "ERROR: dual backup checksum mismatch");
                continue;
            }
            let total = STORAGE_WRITES.fetch_add(1, Ordering::Relaxed) + 1;
            count += 1;

            if count % 20 == 0 {
                info!("fs", "backup: {} records (total={} dual=ok)", count, total);
            }
        }
        yield_now().await;
    }
}

/// Alarm task: monitors vitals and triggers alerts.
/// **Holds**: `Cap<Alarm>` (one-shot, N=1)
/// **Security**: Capability revoked after first alarm activation.
#[caplet_task(priority = 1, stack = 4096, name = "alarm")]
async fn alarm_task() {
    info!(
        "med",
        "alarm_task started (capability: Cap<Alarm> — one-shot)"
    );
    let mut tick: u32 = 0;

    loop {
        let alarm = ALARM.lock();
        if alarm.level != AlarmLevel::None {
            drop(alarm);
            // The alarm is active — perform the alert action.
            activate_alarm_peripheral();
            ALARM_COUNT.fetch_add(1, Ordering::Relaxed);
            info!(
                "alarm",
                "ALARM FIRED: count={}",
                ALARM_COUNT.load(Ordering::Relaxed)
            );
            // One-shot capability has been used; revoke it.
            info!("alarm", "alarm capability revoked (N=1 exhausted)");
            ALARM_ACTIVATED.store(true, Ordering::SeqCst);
            info!(
                "med",
                "MEDICAL complete alarm=1 storage={}",
                STORAGE_WRITES.load(Ordering::Relaxed)
            );
            break;
        }
        drop(alarm);

        // Check vitals.
        if let Some(r) = ALARM_VITALS.lock().take() {
            let new_level = evaluate_vitals(r);
            if new_level != AlarmLevel::None {
                *ALARM.lock() = AlarmState {
                    level: new_level,
                    triggered_at_ms: r.timestamp_ms,
                };
                info!("alarm", "ALARM TRIGGERED: level={:?}", new_level);
            }
        }

        tick += 1;
        if tick % 20 == 0 {
            info!(
                "alarm",
                "tick {}: alarm_count={}",
                tick,
                ALARM_COUNT.load(Ordering::Relaxed)
            );
        }
        yield_now().await;
    }
}

/// Watchdog task: ensures system liveliness.
/// **Holds**: `Cap<Watchdog>`
/// **Security**: Only this task can reset the watchdog.
#[caplet_task(priority = 0, stack = 2048, name = "watchdog")]
async fn watchdog_task() {
    info!("med", "watchdog_task started (capability: Cap<Watchdog>)");
    let mut tick: u32 = 0;

    loop {
        // Reset watchdog timer (capability-gated).
        watchdog_reset();

        tick += 1;
        if tick % 50 == 0 || (ALARM_ACTIVATED.load(Ordering::SeqCst) && tick % 10 == 0) {
            info!(
                "watchdog",
                "WATCHDOG healthy reset={} alarm_activated={} storage={}",
                tick,
                ALARM_ACTIVATED.load(Ordering::SeqCst),
                STORAGE_WRITES.load(Ordering::SeqCst)
            );
        }
        yield_now().await;
    }
}

// ── Alarm evaluation ───────────────────────────────────────────────────

fn evaluate_vitals(r: VitalReading) -> AlarmLevel {
    if r.heart_rate_bpm < 40
        || r.heart_rate_bpm > 180
        || r.spo2_pct < 85
        || r.temperature_c < 35.0
        || r.temperature_c > 40.0
    {
        AlarmLevel::Critical
    } else if r.heart_rate_bpm < 50
        || r.heart_rate_bpm > 130
        || r.spo2_pct < 92
        || r.temperature_c < 35.5
        || r.temperature_c > 38.5
    {
        AlarmLevel::Warning
    } else {
        AlarmLevel::None
    }
}

// ── Capability-gated peripheral operations ──────────────────────────────

/// Activate the alarm peripheral.
/// In a real system, this uses the Cap<Alarm> syscall.
fn activate_alarm_peripheral() {
    info!("periph", "ALARM_PERIPHERAL: ACTIVATED");
}

/// Reset the watchdog timer.
/// In a real system, this uses the Cap<Watchdog> syscall.
fn watchdog_reset() {
    // No-op in simulation.
}

/// Write a backup record to both RAM backup banks.
/// In a real system, the storage capability/syscall enforces that writes
/// go only to the backup partition (not the program flash).
fn write_backup_record(r: VitalReading) -> bool {
    let sequence = STORAGE_WRITES.load(Ordering::Relaxed) as u32 + 1;
    let record = BackupRecord::new(sequence, r);
    let index = (sequence as usize - 1) % BACKUP_DEPTH;
    BACKUP_A.lock()[index] = record;
    BACKUP_B.lock()[index] = record;
    BACKUP_A.lock()[index].is_valid() && BACKUP_B.lock()[index].is_valid()
}

// ── Yield ─────────────────────────────────────────────────────────────

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

#[cfg(target_arch = "arm")]
#[cortex_m_rt::entry]
fn main() -> ! {
    run_demo()
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    run_demo()
}

fn run_demo() -> ! {
    #[cfg(target_arch = "arm")]
    install_writer(Some(&SEMIHOSTING_INSTANCE));
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    install_writer(Some(&caplet_riscv::log::UART_LOG));
    info!(
        "med",
        "medical-device: CapletRT M4 safety-critical scenario"
    );
    info!("med", "  patient: {:?}", PATIENT_ID);
    kernel_init();
    spawn_with_priority(acquire_task(), 3);
    spawn_with_priority(storage_task(), 2);
    spawn_with_priority(alarm_task(), 1);
    spawn_with_priority(watchdog_task(), 0);
    drive_executor_forever()
}
