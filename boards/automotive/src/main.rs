//! Automotive demo: CAN bus monitoring and fault recording.
//!
//! Demonstrates an automotive edge device that:
//!   - Monitors CAN bus messages (engine, braking, steering signals)
//!   - Synchronizes time via IEEE 802.1AS (simplified)
//!   - Records faults to flash (append-only black box)
//!   - Sends alerts via MQTT-SN
//!
//! ## CAN Message IDs (simulated)
//!   0x100 — Engine RPM + temperature
//!   0x200 — Vehicle speed + brake status
//!   0x300 — Steering angle
//!   0x400 — Fault codes
//!
//! ## Capability Model
//! - `Cap<CanBus>` — held ONLY by `can_monitor_task`.
//! - `Cap<Flash>` (append-only) — held ONLY by `fault_recorder_task`.
//! - `Cap<WifiSocket>` — held ONLY by `alert_task`.
//!
//! ## Fault Recording
//! - Append-only log in flash (no overwrite, no delete).
//! - Wrap-around after N records (oldest dropped).
//! - Each record: timestamp (u64) + CAN id (u32) + fault code (u16) + data[8].

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
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};

// ── CAN types ──────────────────────────────────────────────────────────

/// Standard 11-bit CAN message ID.
const CAN_MASK_STD: u32 = 0x7FF;

/// CAN message (simulated).
#[derive(Debug, Clone, Copy)]
struct CanMsg {
    id: u32,
    data: [u8; 8],
    dlc: u8,
    timestamp_ns: u64,
}

/// Parse a CAN data field into meaningful signals.
impl CanMsg {
    fn engine_rpm(&self) -> Option<u16> {
        if self.id == 0x100 { Some(u16::from_le_bytes([self.data[0], self.data[1]])) } else { None }
    }
    fn vehicle_speed_kph(&self) -> Option<u16> {
        if self.id == 0x200 { Some(u16::from_le_bytes([self.data[0], self.data[1]])) } else { None }
    }
    fn brake_pressed(&self) -> Option<bool> {
        if self.id == 0x200 { Some(self.data[2] != 0) } else { None }
    }
    fn fault_code(&self) -> Option<u16> {
        if self.id == 0x400 { Some(u16::from_le_bytes([self.data[0], self.data[1]])) } else { None }
    }
}

// ── Fault record ──────────────────────────────────────────────────────

/// One fault record in the black box log.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct FaultRecord {
    timestamp_ns: u64,
    can_id: u32,
    fault_code: u16,
    _reserved: u16,
    data: [u8; 8],
}

impl Default for FaultRecord {
    fn default() -> Self {
        Self { timestamp_ns: 0, can_id: 0, fault_code: 0, _reserved: 0, data: [0u8; 8] }
    }
}

/// Zero record (suitable for const initialization).
impl FaultRecord {
    const ZERO: Self = Self {
        timestamp_ns: 0,
        can_id: 0,
        fault_code: 0,
        _reserved: 0,
        data: [0u8; 8],
    };
}

// ── Shared state ───────────────────────────────────────────────────────

/// Current vehicle speed (shared by all tasks).
static VEHICLE_SPEED: AtomicU16 = AtomicU16::new(0);

/// Latest engine RPM.
static ENGINE_RPM: AtomicU16 = AtomicU16::new(0);

/// Fault record counter.
static FAULT_RECORDS: AtomicUsize = AtomicUsize::new(0);

/// Alert counter.
static ALERTS_SENT: AtomicUsize = AtomicUsize::new(0);

/// Time synchronization state (IEEE 802.1AS simplified).
static TIME_SYNC: SpinLock<TimeSyncState> = SpinLock::new(TimeSyncState::NEW);

/// Fault log (circular buffer in flash).
const MAX_FAULTS: usize = 256;
static FAULT_LOG: SpinLock<[FaultRecord; MAX_FAULTS]> =
    SpinLock::new([FaultRecord::ZERO; MAX_FAULTS]);
static FAULT_INDEX: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
struct TimeSyncState {
    grandmaster_id: u64,
    synced: bool,
    offset_ns: i64,
    seq_id: u32,
}

impl TimeSyncState {
    const NEW: Self = Self {
        grandmaster_id: 0,
        synced: false,
        offset_ns: 0,
        seq_id: 0,
    };
}

// ── Tasks ─────────────────────────────────────────────────────────────

/// CAN monitor task: reads CAN bus messages.
/// **Holds**: `Cap<CanBus>`
#[caplet_task(priority = 3, stack = 4096, name = "can_monitor")]
async fn can_monitor_task() {
    info!("auto", "can_monitor_task started (capability: Cap<CanBus>)");
    let mut tick: u32 = 0;

    loop {
        // Safety check.
        if !has_can_cap() {
            warn!("auto", "FATAL: missing Cap<CanBus>");
            loop {}
        }

        // Simulate incoming CAN messages.
        let msg = simulate_can_message(tick);
        process_can_message(msg);

        tick = tick.wrapping_add(1);
        if tick % 50 == 0 {
            info!("auto", "CAN: RPM={} SPD={}kph",
                  ENGINE_RPM.load(Ordering::Relaxed),
                  VEHICLE_SPEED.load(Ordering::Relaxed));
        }

        yield_now().await;
    }
}

/// Fault recorder task: appends fault records to flash.
/// **Holds**: `Cap<Flash>` (append-only)
#[caplet_task(priority = 2, stack = 4096, name = "fault_recorder")]
async fn fault_recorder_task() {
    info!("auto", "fault_recorder_task started (capability: Cap<Flash> — append-only)");
    let mut tick: u32 = 0;

    loop {
        // Check for new faults.
        let has_fault = check_fault_condition();
        if has_fault {
            if !has_flash_cap() {
                warn!("auto", "FATAL: missing Cap<Flash>");
                loop {}
            }
            let record = create_fault_record();
            append_fault_record(record);
            FAULT_RECORDS.fetch_add(1, Ordering::Relaxed);

            if tick % 10 == 0 {
                info!("fs", "faults: {} records (total={})",
                      tick, FAULT_RECORDS.load(Ordering::Relaxed));
            }
        }

        tick = tick.wrapping_add(1);
        yield_now().await;
    }
}

/// Time sync task: maintains synchronized time (IEEE 802.1AS simplified).
/// **Priority**: high — time sync must not drift.
#[caplet_task(priority = 2, stack = 2048, name = "time_sync")]
async fn time_sync_task() {
    info!("auto", "time_sync_task started (IEEE 802.1AS simplified)");
    let mut seq_id: u32 = 0;

    loop {
        // Simulate sync interval (every 100 ms).
        let sync_state = &mut *TIME_SYNC.lock();
        sync_state.seq_id = seq_id;
        if !sync_state.synced {
            sync_state.synced = true;
            sync_state.grandmaster_id = 0xABCDEF;
            sync_state.offset_ns = 0;
            info!("ts", "synced: GM={:x} seq={}", sync_state.grandmaster_id, seq_id);
        }

        seq_id = seq_id.wrapping_add(1);
        yield_now().await;
    }
}

/// Alert task: sends MQTT-SN alerts when faults are detected.
/// **Holds**: `Cap<WifiSocket>`
#[caplet_task(priority = 1, stack = 4096, name = "alert")]
async fn alert_task() {
    info!("auto", "alert_task started (capability: Cap<WifiSocket>)");
    let mut tick: u32 = 0;

    loop {
        if FAULT_RECORDS.load(Ordering::Relaxed) > 0 {
            let payload = build_alert_payload();
            info!("mqtt-sn", "PUBLISH automotive/fault len={}", payload.len());
            ALERTS_SENT.fetch_add(1, Ordering::Relaxed);
        }

        tick += 1;
        if tick >= 5 { break; }

        yield_now().await;
    }
}

// ── CAN processing ─────────────────────────────────────────────────────

fn has_can_cap() -> bool { true } // simulate capability check
fn has_flash_cap() -> bool { true }

fn simulate_can_message(tick: u32) -> CanMsg {
    match tick % 4 {
        0 => CanMsg {
            id: 0x100,
            data: [(tick % 8000) as u8, ((tick % 8000) >> 8) as u8, 0, 0, 0, 0, 0, 0],
            dlc: 8,
            timestamp_ns: (tick as u64) * 1_000_000,
        },
        1 => CanMsg {
            id: 0x200,
            data: [(tick % 220) as u8, ((tick % 220) >> 8) as u8, 0, 0, 0, 0, 0, 0],
            dlc: 8,
            timestamp_ns: (tick as u64) * 1_000_000,
        },
        2 => CanMsg {
            id: 0x300,
            data: [(tick % 360) as u8, 0, 0, 0, 0, 0, 0, 0],
            dlc: 4,
            timestamp_ns: (tick as u64) * 1_000_000,
        },
        _ => CanMsg {
            id: 0x400,
            data: [0, 0, 0, 0, 0, 0, 0, 0],
            dlc: 0,
            timestamp_ns: (tick as u64) * 1_000_000,
        },
    }
}

fn process_can_message(msg: CanMsg) {
    if let Some(rpm) = msg.engine_rpm() {
        ENGINE_RPM.store(rpm, Ordering::Relaxed);
    }
    if let Some(spd) = msg.vehicle_speed_kph() {
        VEHICLE_SPEED.store(spd, Ordering::Relaxed);
    }
}

fn check_fault_condition() -> bool {
    let rpm = ENGINE_RPM.load(Ordering::Relaxed);
    let spd = VEHICLE_SPEED.load(Ordering::Relaxed);
    rpm > 7000 || spd > 200
}

fn create_fault_record() -> FaultRecord {
    let synced = {
        let ts = TIME_SYNC.lock();
        ts.synced
    };
    let offset_ns = if synced { TIME_SYNC.lock().offset_ns as u64 } else { 0 };
    FaultRecord {
        timestamp_ns: 0, // would be from global time
        can_id: 0x400,
        fault_code: 1,
        _reserved: 0,
        data: [0u8; 8],
    }
}

fn append_fault_record(record: FaultRecord) {
    let idx = FAULT_INDEX.fetch_add(1, Ordering::Relaxed) % MAX_FAULTS;
    FAULT_LOG.lock()[idx] = record;
    info!("blackbox", "record #{} written: id=0x{:x} code={}",
          idx, record.can_id, record.fault_code);
}

fn build_alert_payload() -> heapless::String<48> {
    use heapless::String;
    let mut s: String<48> = String::new();
    let _ = s.push_str("FAULT:RPM=");
    let rpm = ENGINE_RPM.load(Ordering::Relaxed);
    let _ = itoa_u16(&mut s, rpm);
    s
}

fn itoa_u16(s: &mut heapless::String<48>, n: u16) {
    let mut buf = [0u8; 5];
    let mut i = 0;
    let mut v = n as u32;
    if v == 0 { buf[0] = b'0'; i = 1; }
    while v > 0 { buf[i] = b'0' + (v % 10) as u8; v /= 10; i += 1; }
    buf[..i].reverse();
    for &b in &buf[..i] { let _ = s.push(b as char); }
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
    info!("auto", "automotive: CapletRT M4 automotive scenario");
    info!("auto", "  CAN: 0x100 rpm 0x200 speed 0x300 steer 0x400 faults");
    kernel_init();
    spawn_with_priority(can_monitor_task(), 3);
    spawn_with_priority(fault_recorder_task(), 2);
    spawn_with_priority(time_sync_task(), 2);
    spawn_with_priority(alert_task(), 1);
    drive_executor_forever()
}

/// Panic handler for host builds.
#[cfg(any(feature = "target-none", not(target_os = "none")))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
