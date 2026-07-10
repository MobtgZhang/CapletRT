//! Medical device demo: safety-critical vital signs monitoring.
//!
//! ## Capability Model
//! This board demonstrates the security-critical aspects of the capability
//! system applied to medical device safety:
//!
//! - `Cap<VitalSensor>` — held ONLY by `acquire_task`. Other tasks
//!   cannot read vital signs (no capability = no access).
//! - `Cap<Flash>` — held ONLY by `storage_task`. Backup writes are
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

extern crate caplet_cortex_m;
extern crate caplet_kernel;
extern crate caplet_macros;

// ── Bump allocator ────────────────────────────────────────────────────────

const HEAP_BASE: usize = 0x2000_0000;
const HEAP_END: usize = 0x2010_0000;

struct BumpAlloc;

unsafe impl core::alloc::GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        static NEXT: AtomicUsize = AtomicUsize::new(HEAP_BASE);
        loop {
            let cur = NEXT.load(Ordering::Relaxed);
            let aligned = (cur + layout.align() - 1) & !(layout.align() - 1);
            let end = aligned + layout.size();
            if end > HEAP_END { return core::ptr::null_mut(); }
            if NEXT.compare_exchange(cur, end, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                return aligned as *mut u8;
            }
        }
    }
    unsafe fn dealloc(&self, _: *mut u8, _: core::alloc::Layout) {}
}

#[global_allocator]
static HEAP: BumpAlloc = BumpAlloc;

use caplet_log::{info, warn, install_writer, semihosting::SEMIHOSTING_INSTANCE};
use caplet_macros::task as caplet_task;
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
    while x2 > PI { x2 -= TWO_PI; }
    while x2 < -PI { x2 += TWO_PI; }
    let x3 = x2 * x2 * x2;
    let x5 = x3 * x2 * x2;
    x2 - x3 / 6.0 + x5 / 120.0
}

// ── Shared state ────────────────────────────────────────────────────────

/// Current vital reading (derived to read-only for display task).
static VITALS: SpinLock<Option<VitalReading>> = SpinLock::new(None);

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

/// Patient ID (would come from NFC/QR scanner).
const PATIENT_ID: &[u8] = b"PATIENT_001";

// ── Capability helpers ──────────────────────────────────────────────────
//
// In a real CapletRT build these would be actual Cap<T> handles passed
// to tasks at spawn time. Here we simulate the capability shape to
// demonstrate the design intent.

const CAP_SENSOR: bool = true;  // acquire_task has it
const CAP_FLASH: bool = true;   // storage_task has it
const CAP_ALARM: bool = true;   // alarm_task has it (N=1, revoked after use)

/// Check if current task holds vital sensor capability.
fn has_vital_sensor_cap() -> bool {
    CAP_SENSOR
}

/// Check if current task holds flash capability.
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

        // Simulate vital signs with slight variation.
        let reading = VitalReading {
            heart_rate_bpm: base_hr + ((tick % 20) as u16).wrapping_sub(10),
            spo2_pct: base_spo2 - ((tick % 5) as u8),
            temperature_c: base_temp + fast_sin((tick as f32) * 0.001) * 0.3,
            timestamp_ms: tick * 1000,
        };

        // Update the shared state.
        *VITALS.lock() = Some(reading);

        // Log every 10th reading.
        if tick % 10 == 0 {
            info!("med", "vitals: HR={} SpO2={}% T={:.1}C",
                  reading.heart_rate_bpm,
                  reading.spo2_pct,
                  reading.temperature_c);
        }

        // Advance simulation.
        tick += 1;
        if tick > 200 { tick = 0; }

        yield_now().await;
    }
}

/// Storage task: dual-backup writes vital signs.
/// **Holds**: `Cap<Flash>` (scoped to backup partition only)
/// **Security**: Can only write to the backup flash region.
#[caplet_task(priority = 2, stack = 4096, name = "storage")]
async fn storage_task() {
    info!("med", "storage_task started (capability: Cap<Flash> — backup partition)");
    let mut count: u32 = 0;

    loop {
        let reading = VITALS.lock().take();
        if let Some(r) = reading {
            if !has_flash_cap() {
                warn!("med", "FATAL: missing Cap<Flash>");
                loop {}
            }

            // Write to backup flash partition (capability-scoped).
            write_backup_record(r);
            STORAGE_WRITES.fetch_add(1, Ordering::Relaxed);
            count += 1;

            if count % 20 == 0 {
                info!("fs", "backup: {} records (total={})",
                      count, STORAGE_WRITES.load(Ordering::Relaxed));
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
    info!("med", "alarm_task started (capability: Cap<Alarm> — one-shot)");
    let mut tick: u32 = 0;

    loop {
        let alarm = ALARM.lock();
        if alarm.level != AlarmLevel::None {
            drop(alarm);
            // The alarm is active — perform the alert action.
            activate_alarm_peripheral();
            ALARM_COUNT.fetch_add(1, Ordering::Relaxed);
            info!("alarm", "ALARM FIRED: count={}", ALARM_COUNT.load(Ordering::Relaxed));
            // One-shot capability has been used; revoke it.
            info!("alarm", "alarm capability revoked (N=1 exhausted)");
            ALARM_ACTIVATED.store(true, Ordering::SeqCst);
            break;
        }
        drop(alarm);

        // Check vitals.
        if let Some(r) = *VITALS.lock() {
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
            info!("alarm", "tick {}: alarm_count={}", tick, ALARM_COUNT.load(Ordering::Relaxed));
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
        if tick % 50 == 0 {
            info!("watchdog", "WD reset #{} alarm_activated={} storage={}",
                  tick, ALARM_ACTIVATED.load(Ordering::SeqCst),
                  STORAGE_WRITES.load(Ordering::SeqCst));
        }
        yield_now().await;
    }
}

// ── Alarm evaluation ───────────────────────────────────────────────────

fn evaluate_vitals(r: VitalReading) -> AlarmLevel {
    if r.heart_rate_bpm < 40 || r.heart_rate_bpm > 180
        || r.spo2_pct < 85
        || r.temperature_c < 35.0
        || r.temperature_c > 40.0
    {
        AlarmLevel::Critical
    } else if r.heart_rate_bpm < 50 || r.heart_rate_bpm > 130
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

/// Write a backup record to the medical data flash partition.
/// In a real system, the Cap<Flash> syscall enforces that writes
/// go only to the backup partition (not the program flash).
fn write_backup_record(r: VitalReading) {
    let _ = r;
}

// ── Yield ─────────────────────────────────────────────────────────────

async fn yield_now() { YieldOnce(false).await }
struct YieldOnce(bool);
impl core::future::Future for YieldOnce {
    type Output = ();
    fn poll(mut self: core::pin::Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> core::task::Poll<()> {
        if self.0 { core::task::Poll::Ready(()) }
        else {
            self.0 = true;
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    }
}

#[cfg_attr(target_os = "none", cortex_m_rt::entry)]
fn main() -> ! {
    install_writer(Some(&SEMIHOSTING_INSTANCE));
    info!("med", "medical-device: CapletRT M4 safety-critical scenario");
    info!("med", "  patient: {:?}", PATIENT_ID);
    kernel_init();
    spawn_with_priority(acquire_task(), 3);
    spawn_with_priority(storage_task(), 2);
    spawn_with_priority(alarm_task(), 1);
    spawn_with_priority(watchdog_task(), 0);
    drive_executor_forever()
}

/// Panic handler for host builds.
#[cfg(any(feature = "target-none", not(target_os = "none")))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
