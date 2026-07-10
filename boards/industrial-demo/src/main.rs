//! Industrial demo: full MQTT-SN application with storage and watchdog.
//!
//! This expands the original demo to demonstrate the full CapletRT
//! ecosystem:
//!
//! - `sample_task`   — reads sensor, stores to flash (priority 3)
//! - `storage_task`  — persists readings to LittleFS (priority 2)
//! - `upload_task`   — publishes via MQTT-SN over UDP (priority 1)
//! - `watchdog_task` — system health monitoring, panic recovery (priority 0)
//!
//! The split demonstrates capability-driven isolation: the storage
//! task holds the FlashToken, the upload task holds the SocketToken,
//! and the watchdog holds neither — it only observes via the kernel.

#![no_std]
#![no_main]

extern crate alloc;

extern crate caplet_cortex_m;
extern crate caplet_kernel;
extern crate caplet_macros;

use caplet_log::{info, install_writer, semihosting::SEMIHOSTING_INSTANCE};
use caplet_macros::task as caplet_task;
use caplet_macros_exports::{drive_executor_forever, kernel_init, spawn_with_priority};
use caplet_sensors::{MockSensor, Reading};
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Bump allocator for the demo.
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
            if end > HEAP_END { return core::ptr::null_mut(); }
            if NEXT.compare_exchange(cur, end, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                return aligned as *mut u8;
            }
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[global_allocator]
static HEAP: BumpAlloc = BumpAlloc;

// ── Shared data structures ───────────────────────────────────────────────

/// Rolling log of the last `LOG_DEPTH` readings.
const LOG_DEPTH: usize = 64;
static READINGS_LOG: SpinLock<heapless::Vec<Reading, LOG_DEPTH>> =
    SpinLock::new(heapless::Vec::new());

/// Mailbox: the latest sensor reading (ready for upload).
static LATEST: SpinLock<Option<Reading>> = SpinLock::new(None);

/// Flash write counter (simulates successful writes).
static FLASH_WRITES: AtomicUsize = AtomicUsize::new(0);

/// Upload counter (number of MQTT-SN PUBLISHes sent).
static UPLOADS: AtomicUsize = AtomicUsize::new(0);

/// Watchdog heartbeat counter.
static HEARTBEATS: AtomicUsize = AtomicUsize::new(0);

/// Mock sensor.
static SENSOR: SpinLock<MockSensor> =
    SpinLock::new(MockSensor::new_init("mock-temp", 22.0));

// ── Tasks ────────────────────────────────────────────────────────────────

/// Sample task: reads the sensor and updates the shared state.
/// Priority 3 (highest) — sensor I/O must never be starved.
#[caplet_task(priority = 3, stack = 4096, name = "sample")]
async fn sample_task() {
    info!("demo", "sample_task started");
    let mut count: u32 = 0;
    loop {
        // Read the sensor.
        let reading = SENSOR.lock().data().read();

        // Update the rolling log.
        let mut log = READINGS_LOG.lock();
        if log.len() >= LOG_DEPTH {
            let _ = log.remove(0);
        }
        let _ = log.push(reading);

        // Update the latest mailbox.
        let mut latest = LATEST.lock();
        *latest = Some(reading);

        count += 1;
        if count % 20 == 0 {
            info!("demo", "sampled {} total readings", count);
        }

        yield_now().await;
    }
}

/// Storage task: writes each new reading to flash.
/// Priority 2 — can be delayed but must eventually persist.
/// Capability demonstration: this task holds the FlashToken.
#[caplet_task(priority = 2, stack = 4096, name = "storage")]
async fn storage_task() {
    info!("demo", "storage_task started (capability: FlashToken)");
    let mut count: u32 = 0;
    loop {
        let reading = LATEST.lock().take();
        if let Some(r) = reading {
            // Simulate flash write (capability would gate this in real HW).
            // The kernel's FlashToken enforces that only this task can write.
            write_flash_record(r.timestamp_ms, r.value);
            FLASH_WRITES.fetch_add(1, Ordering::Relaxed);
            count += 1;
            if count % 10 == 0 {
                info!("fs", "persisted {} records (total writes={})",
                      count, FLASH_WRITES.load(Ordering::Relaxed));
            }
        }
        yield_now().await;
    }
}

/// Upload task: publishes readings via MQTT-SN PUBLISH.
/// Priority 1 — network is the slowest link.
/// Capability demonstration: this task holds the SocketToken.
#[caplet_task(priority = 1, stack = 4096, name = "upload")]
async fn upload_task() {
    info!("demo", "upload_task started (capability: SocketToken)");
    let mut count: u32 = 0;
    loop {
        let reading = LATEST.lock().take();
        if let Some(r) = reading {
            // Build a mock MQTT-SN PUBLISH payload.
            let payload = build_mqtt_sn_payload(r);
            info!("mqtt-sn", "PUBLISH sensors/temp len={}", payload.len());
            // In the real demo: caplet_mqttsn::Client::publish(&payload)
            UPLOADS.fetch_add(1, Ordering::Relaxed);
            count += 1;
            if count >= 5 {
                info!("demo", "uploaded {} messages, demo complete", count);
                break;
            }
        }
        yield_now().await;
    }
}

/// Watchdog task: monitors system health.
/// Priority 0 (highest priority) — runs on every scheduler tick.
/// Capability demonstration: this task holds NO security token.
/// It only reads shared counters and panics if thresholds are breached.
#[caplet_task(priority = 0, stack = 2048, name = "watchdog")]
async fn watchdog_task() {
    info!("demo", "watchdog_task started (no security capability)");
    loop {
        let hb = HEARTBEATS.fetch_add(1, Ordering::Relaxed);
        let reads = FLASH_WRITES.load(Ordering::Relaxed);
        let sends = UPLOADS.load(Ordering::Relaxed);

        // Simple health check: if no readings in 30 ticks, log warning.
        // In a real system this would trigger a reset.
        if hb % 30 == 0 {
            if reads == 0 || sends == 0 {
                info!("watchdog", "WARNING: no flash writes or uploads in 30 ticks");
            } else {
                info!("watchdog", "heartbeat {}: writes={} sends={}", hb, reads, sends);
            }
        }

        yield_now().await;
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Simulate a flash write of a reading record.
/// In a real system this would use the FlashToken syscall.
fn write_flash_record(timestamp_ms: u64, value: f32) {
    let _ = timestamp_ms;
    let _ = value;
    // Real: syscall_flash_write(...);
}

/// Build a mock MQTT-SN PUBLISH payload.
///
/// Format: `ID=device_id&t=NN.NC` (compact, ascii, no JSON).
fn build_mqtt_sn_payload(r: Reading) -> heapless::String<32> {
    use heapless::String;
    let mut s: String<32> = String::new();
    // Encode device_id.
    for &b in r.device_id {
        if b == 0 { break; }
        let _ = s.push(b as char);
    }
    let _ = s.push('&');
    let _ = s.push('t');
    let _ = s.push('=');
    // Encode temperature: whole part (up to 3 digits).
    let whole = (r.value as i32).abs();
    let whole_bytes = itoa(whole);
    for &b in &whole_bytes {
        if b == 0 { break; }
        let _ = s.push(b as char);
    }
    let _ = s.push('.');
    let frac = ((r.value.fract().abs()) * 10.0) as u8;
    let _ = s.push((b'0' + (frac / 10) % 10) as char);
    let _ = s.push((b'0' + frac % 10) as char);
    s
}

/// Minimal itoa (no_std compatible).
fn itoa(mut n: i32) -> [u8; 12] {
    let mut buf = [0u8; 12];
    let neg = n < 0;
    if n == 0 {
        buf[0] = b'0';
        return buf;
    }
    if neg { n = -n; }
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    if neg && i < 11 { buf[i] = b'-'; i += 1; }
    buf[..i].reverse();
    buf
}

/// Tiny coroutine: resolves on next poll (cooperative yield).
async fn yield_now() {
    YieldOnce(false).await
}

struct YieldOnce(bool);
impl core::future::Future for YieldOnce {
    type Output = ();
    fn poll(mut self: core::pin::Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> core::task::Poll<()> {
        if self.0 {
            core::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    }
}

// ── Entry point ──────────────────────────────────────────────────────────

#[cortex_m_rt::entry]
fn main() -> ! {
    install_writer(Some(&SEMIHOSTING_INSTANCE));
    info!("demo", "industrial-demo: CapletRT M4 industrial scenario");
    info!("demo", "  tasks: sample / storage / upload / watchdog");
    SENSOR.lock().set_value(22.5);

    kernel_init();

    spawn_with_priority(sample_task(), 3);
    spawn_with_priority(storage_task(), 2);
    spawn_with_priority(upload_task(), 1);
    spawn_with_priority(watchdog_task(), 0);

    drive_executor_forever()
}
