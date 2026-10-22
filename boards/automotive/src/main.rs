//! Automotive demo: CAN bus monitoring and fault recording.
//!
//! Demonstrates an automotive edge device that:
//!   - Monitors CAN bus messages (engine, braking, steering signals)
//!   - Synchronizes time via IEEE 802.1AS (simplified)
//!   - Records faults to an append-only RAM-backed black box
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
//! - storage capability (append-only) — held ONLY by `fault_recorder_task`.
//! - `Cap<WifiSocket>` — held ONLY by `alert_task`.
//!
//! ## Fault Recording
//! - Append-only log in RAM (no overwrite, no delete). The MPS2 model
//!   has no persistent flash controller, so this does not survive power
//!   loss; the record format is ready for a real `FlashStorage` backend.
//! - Wrap-around after N records (oldest dropped).
//! - Each record: timestamp (u64) + CAN id (u32) + fault code (u16) + data[8].

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
use caplet_mqttsn::e2e::MockBroker;
use caplet_mqttsn::{msgtype, Client, ReturnCode};
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicUsize, Ordering};

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
        if self.id == 0x100 {
            Some(u16::from_le_bytes([self.data[0], self.data[1]]))
        } else {
            None
        }
    }
    fn vehicle_speed_kph(&self) -> Option<u16> {
        if self.id == 0x200 {
            Some(u16::from_le_bytes([self.data[0], self.data[1]]))
        } else {
            None
        }
    }
    fn brake_pressed(&self) -> Option<bool> {
        if self.id == 0x200 {
            Some(self.data[2] != 0)
        } else {
            None
        }
    }
    fn fault_code(&self) -> Option<u16> {
        if self.id == 0x400 {
            Some(u16::from_le_bytes([self.data[0], self.data[1]]))
        } else {
            None
        }
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
        Self {
            timestamp_ns: 0,
            can_id: 0,
            fault_code: 0,
            _reserved: 0,
            data: [0u8; 8],
        }
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

/// Latest CAN fault code (0 means no active fault).
static FAULT_CODE: AtomicU16 = AtomicU16::new(0);

/// Current CAN simulation tick.
static CAN_TICKS: AtomicU32 = AtomicU32::new(0);

/// Rising-edge latch so one fault event produces exactly one black-box record.
static FAULT_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Fault record counter.
static FAULT_RECORDS: AtomicUsize = AtomicUsize::new(0);

/// Alert counter.
static ALERTS_SENT: AtomicUsize = AtomicUsize::new(0);

/// Time synchronization state (IEEE 802.1AS simplified).
static TIME_SYNC: SpinLock<TimeSyncState> = SpinLock::new(TimeSyncState::NEW);

/// Fault log (append-only circular buffer in RAM).
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
        CAN_TICKS.store(tick, Ordering::Relaxed);

        tick = tick.wrapping_add(1);
        if tick % 50 == 0 {
            info!(
                "auto",
                "CAN: RPM={} SPD={}kph",
                ENGINE_RPM.load(Ordering::Relaxed),
                VEHICLE_SPEED.load(Ordering::Relaxed)
            );
        }

        yield_now().await;
    }
}

/// Fault recorder task: appends fault records to the RAM black box.
/// **Holds**: storage capability (append-only)
#[caplet_task(priority = 2, stack = 4096, name = "fault_recorder")]
async fn fault_recorder_task() {
    info!(
        "auto",
        "fault_recorder_task started (capability: storage — append-only, RAM-backed)"
    );
    let mut tick: u32 = 0;

    loop {
        // Check for new faults.
        let has_fault = check_fault_condition();
        // Always update the latch, including on the falling edge.  Using a
        // short-circuit here (`has_fault && swap(...)`) leaves the latch set
        // forever after the first fault and suppresses every later record.
        let was_active = FAULT_ACTIVE.swap(has_fault, Ordering::Relaxed);
        let rising_edge = has_fault && !was_active;
        if rising_edge {
            if !has_flash_cap() {
                warn!("auto", "FATAL: missing Cap<Flash>");
                loop {}
            }
            let record = create_fault_record();
            append_fault_record(record);
            FAULT_RECORDS.fetch_add(1, Ordering::Relaxed);

            if tick % 10 == 0 {
                info!(
                    "fs",
                    "faults: {} records (total={})",
                    tick,
                    FAULT_RECORDS.load(Ordering::Relaxed)
                );
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
        let became_synced = {
            let mut sync_state = TIME_SYNC.lock();
            sync_state.seq_id = seq_id;
            if !sync_state.synced {
                sync_state.synced = true;
                sync_state.grandmaster_id = 0xABCDEF;
                sync_state.offset_ns = 0;
                true
            } else {
                false
            }
        };
        if became_synced {
            let (grandmaster_id, offset_ns) = {
                let sync_state = TIME_SYNC.lock();
                (sync_state.grandmaster_id, sync_state.offset_ns)
            };
            info!(
                "ts",
                "synced: GM={:x} offset={}ns seq={}", grandmaster_id, offset_ns, seq_id
            );
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
    let mut client = Client::new();
    let mut broker = MockBroker::new();

    let search = Client::build_searchgw(1);
    log_packet("SEARCHGW", &search);
    let gwinfo = match broker.exchange(&search) {
        Some(packet) => packet,
        None => {
            info!("mqtt-sn", "ERROR: gateway did not answer SEARCHGW");
            return;
        }
    };
    log_packet("GWINFO", &gwinfo);

    let connect = client.begin_connect("caplet-automotive", 60, true);
    log_packet("CONNECT", &connect);
    let connack = match broker.exchange(&connect) {
        Some(packet) => packet,
        None => {
            info!("mqtt-sn", "ERROR: gateway did not answer CONNECT");
            return;
        }
    };
    log_packet("CONNACK", &connack);
    if client.apply_connack(&connack) != Some(ReturnCode::Accepted) {
        info!("mqtt-sn", "ERROR: CONNACK rejected");
        return;
    }

    let register =
        Client::build_register_with_msg_id(0, client.next_message_id(), "automotive/fault");
    log_packet("REGISTER", &register);
    let regack = match broker.exchange(&register) {
        Some(packet) => packet,
        None => {
            info!("mqtt-sn", "ERROR: gateway did not answer REGISTER");
            return;
        }
    };
    log_packet("REGACK", &regack);
    if client.apply_regack(&regack) != Some(ReturnCode::Accepted) {
        info!("mqtt-sn", "ERROR: REGACK rejected");
        return;
    }
    client.qos = 1;
    info!(
        "mqtt-sn",
        "ACTIVE topic={} topic_id=0x{:04x}",
        broker.topic_name(),
        client.topic_id
    );

    let mut published: usize = 0;

    loop {
        let recorded = FAULT_RECORDS.load(Ordering::Relaxed);
        if recorded > published {
            let payload = build_alert_payload();
            let publish = match client.publish(payload.as_bytes(), false, published as u32 * 100) {
                Some(packet) => packet,
                None => {
                    info!("mqtt-sn", "ERROR: publish state machine rejected packet");
                    return;
                }
            };
            log_packet("PUBLISH", &publish);
            let puback = match broker.exchange(&publish) {
                Some(packet) => packet,
                None => {
                    info!("mqtt-sn", "ERROR: gateway did not answer PUBLISH");
                    return;
                }
            };
            log_packet("PUBACK", &puback);
            if client.apply_puback(&puback) != Some(ReturnCode::Accepted) {
                info!("mqtt-sn", "ERROR: PUBACK rejected or mismatched");
                return;
            }
            if client.pending_message_id().is_some() {
                info!("mqtt-sn", "ERROR: QoS1 publish still pending after PUBACK");
                return;
            }
            info!(
                "mqtt-sn",
                "PUBLISH topic={} qos={} payload={} bytes={} acknowledged=true",
                broker.topic_name(),
                broker.publish_qos(),
                payload,
                broker.published_payload().len()
            );
            published += 1;
            ALERTS_SENT.fetch_add(1, Ordering::Relaxed);
            if published >= 3 {
                let disconnect = Client::build_disconnect();
                log_packet("DISCONNECT", &disconnect);
                let _ = broker.exchange(&disconnect);
                info!(
                    "auto",
                    "ALERTS complete={} packets={}",
                    published,
                    broker.packet_count()
                );
                info!(
                    "auto",
                    "AUTOMOTIVE complete faults={} alerts={} blackbox={}",
                    recorded,
                    ALERTS_SENT.load(Ordering::Relaxed),
                    FAULT_RECORDS.load(Ordering::Relaxed)
                );
                break;
            }
        }

        yield_now().await;
    }
}

// ── CAN processing ─────────────────────────────────────────────────────

fn has_can_cap() -> bool {
    true
} // simulate capability check
fn has_flash_cap() -> bool {
    true
}

fn simulate_can_message(tick: u32) -> CanMsg {
    match tick % 4 {
        0 => CanMsg {
            id: 0x100,
            data: [
                if (20..24).contains(&tick) {
                    0xB8
                } else {
                    (800 + (tick % 100)) as u8
                },
                if (20..24).contains(&tick) {
                    0x1D
                } else {
                    ((800 + (tick % 100)) >> 8) as u8
                },
                0,
                0,
                0,
                0,
                0,
                0,
            ],
            dlc: 8,
            timestamp_ns: (tick as u64) * 1_000_000,
        },
        1 => CanMsg {
            id: 0x200,
            data: [
                if (44..48).contains(&tick) { 245 } else { 60 },
                0,
                if (44..48).contains(&tick) { 0 } else { 1 },
                0,
                0,
                0,
                0,
                0,
            ],
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
            data: if (68..72).contains(&tick) {
                [0x01, 0x01, 0, 0, 0, 0, 0, 0]
            } else {
                [0, 0, 0, 0, 0, 0, 0, 0]
            },
            dlc: 8,
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
    if let Some(code) = msg.fault_code() {
        FAULT_CODE.store(code, Ordering::Relaxed);
    }
}

fn check_fault_condition() -> bool {
    let rpm = ENGINE_RPM.load(Ordering::Relaxed);
    let spd = VEHICLE_SPEED.load(Ordering::Relaxed);
    let code = FAULT_CODE.load(Ordering::Relaxed);
    rpm > 7000 || spd > 200 || code != 0
}

fn create_fault_record() -> FaultRecord {
    let offset_ns = {
        let ts = TIME_SYNC.lock();
        if ts.synced {
            ts.offset_ns as u64
        } else {
            0
        }
    };
    let rpm = ENGINE_RPM.load(Ordering::Relaxed);
    let spd = VEHICLE_SPEED.load(Ordering::Relaxed);
    let code = FAULT_CODE.load(Ordering::Relaxed);
    let can_id = if code != 0 {
        0x400
    } else if rpm > 7000 {
        0x100
    } else {
        0x200
    };
    FaultRecord {
        timestamp_ns: (CAN_TICKS.load(Ordering::Relaxed) as u64) * 1_000_000 + offset_ns,
        can_id,
        fault_code: if code != 0 { code } else { 1 },
        _reserved: 0,
        data: [
            (rpm & 0xff) as u8,
            (rpm >> 8) as u8,
            (spd & 0xff) as u8,
            (spd >> 8) as u8,
            0,
            0,
            0,
            0,
        ],
    }
}

fn append_fault_record(record: FaultRecord) {
    let idx = FAULT_INDEX.fetch_add(1, Ordering::Relaxed) % MAX_FAULTS;
    FAULT_LOG.lock()[idx] = record;
    info!(
        "blackbox",
        "record #{} written: id=0x{:x} code={}", idx, record.can_id, record.fault_code
    );
}

fn build_alert_payload() -> heapless::String<48> {
    use heapless::String;
    let mut s: String<48> = String::new();
    let _ = s.push_str("FAULT code=");
    let _ = itoa_u16(&mut s, FAULT_CODE.load(Ordering::Relaxed));
    let _ = s.push_str(" rpm=");
    let rpm = ENGINE_RPM.load(Ordering::Relaxed);
    let _ = itoa_u16(&mut s, rpm);
    let _ = s.push_str(" spd=");
    let _ = itoa_u16(&mut s, VEHICLE_SPEED.load(Ordering::Relaxed));
    s
}

fn itoa_u16(s: &mut heapless::String<48>, n: u16) {
    let mut buf = [0u8; 5];
    let mut i = 0;
    let mut v = n as u32;
    if v == 0 {
        buf[0] = b'0';
        i = 1;
    }
    while v > 0 {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    buf[..i].reverse();
    for &b in &buf[..i] {
        let _ = s.push(b as char);
    }
}

fn packet_type_name(msg_type: u8) -> &'static str {
    match msg_type {
        msgtype::SEARCHGW => "SEARCHGW",
        msgtype::GWINFO => "GWINFO",
        msgtype::CONNECT => "CONNECT",
        msgtype::CONNACK => "CONNACK",
        msgtype::REGISTER => "REGISTER",
        msgtype::REGACK => "REGACK",
        msgtype::PUBLISH => "PUBLISH",
        msgtype::PUBACK => "PUBACK",
        msgtype::DISCONNECT => "DISCONNECT",
        _ => "UNKNOWN",
    }
}

fn log_packet(expected: &str, packet: &[u8]) {
    use core::fmt::Write as _;
    let actual = packet
        .get(1)
        .copied()
        .map(packet_type_name)
        .unwrap_or("TRUNCATED");
    let mut wire = heapless::String::<128>::new();
    for byte in packet {
        let _ = write!(&mut wire, "{:02x}", byte);
    }
    info!(
        "mqtt-sn",
        "TX/RX expected={} type={} code=0x{:02x} len={} wire={}",
        expected,
        actual,
        packet.get(1).copied().unwrap_or(0),
        packet.len(),
        wire
    );
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
    info!("auto", "automotive: CapletRT M4 automotive scenario");
    info!(
        "auto",
        "  CAN: 0x100 rpm 0x200 speed 0x300 steer 0x400 faults"
    );
    kernel_init();
    spawn_with_priority(can_monitor_task(), 3);
    spawn_with_priority(fault_recorder_task(), 2);
    spawn_with_priority(time_sync_task(), 2);
    spawn_with_priority(alert_task(), 1);
    drive_executor_forever()
}
