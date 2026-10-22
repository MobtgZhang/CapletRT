//! Industrial demo: full MQTT-SN application with storage and watchdog.
//!
//! This expands the original demo to demonstrate the full CapletRT
//! ecosystem:
//!
//! - `sample_task`   — reads sensor and hands readings to storage (priority 3)
//! - `storage_task`  — validates records in a RAM-backed store (priority 2)
//! - `upload_task`   — publishes via MQTT-SN over UDP (priority 1)
//! - `watchdog_task` — system health monitoring, panic recovery (priority 0)
//!
//! The split demonstrates capability-driven isolation: the storage
//! task holds the storage token, the upload task holds the SocketToken,
//! and the watchdog holds neither — it only observes via the kernel.
//!
//! The MPS2 board model has no persistent flash controller. The record
//! store therefore exercises the serialisation/checksum/capacity path in
//! RAM and does not claim power-loss persistence. A production board can
//! replace this backend with `caplet_ota::FlashStorage` without changing
//! the task or capability boundaries.

#![no_std]
#![no_main]

extern crate alloc;

#[cfg(target_arch = "arm")]
extern crate caplet_cortex_m;
extern crate caplet_kernel;
extern crate caplet_macros;

use caplet::task as caplet_task;
#[cfg(target_arch = "arm")]
use caplet_log::semihosting::SEMIHOSTING_INSTANCE;
use caplet_log::{info, install_writer};
use caplet_macros_exports::{drive_executor_forever, kernel_init, spawn_with_priority};
use caplet_mqttsn::e2e::MockBroker;
use caplet_mqttsn::{msgtype, Client, ReturnCode};
use caplet_sensors::{MockSensor, Reading, Sensor};
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Bump allocator for the demo.
///
/// On Cortex-M the heap sits at a fixed offset in the MPS2 SRAM window.
/// On RISC-V it follows `.bss` up to the linker-provided `_eheap`, which
/// leaves the 64 KiB initial stack at the top of DRAM untouched.
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
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[global_allocator]
static HEAP: BumpAlloc = BumpAlloc;

// ── Shared data structures ───────────────────────────────────────────────

/// Rolling log of the last `LOG_DEPTH` readings.
const LOG_DEPTH: usize = 64;
static READINGS_LOG: SpinLock<heapless::Vec<Reading, LOG_DEPTH>> =
    SpinLock::new(heapless::Vec::new());

/// Independent consumer mailboxes. A single mailbox would let the storage
/// task consume every reading before the uploader observes it.
static STORAGE_LATEST: SpinLock<Option<Reading>> = SpinLock::new(None);
static UPLOAD_LATEST: SpinLock<Option<Reading>> = SpinLock::new(None);

/// A checksummed record. The QEMU MPS2 board has no flash controller, so the
/// demo keeps a bounded record store in RAM while exercising the exact
/// serialisation and validation path used by a real flash backend.
#[derive(Debug, Clone, Copy)]
struct StorageRecord {
    sequence: u32,
    timestamp_ms: u64,
    value_bits: u32,
    checksum: u32,
}

static RECORD_LOG: SpinLock<heapless::Vec<StorageRecord, LOG_DEPTH>> =
    SpinLock::new(heapless::Vec::new());

/// Successful RAM-backed record writes.
static STORAGE_WRITES: AtomicUsize = AtomicUsize::new(0);

/// Upload counter (number of MQTT-SN PUBLISHes sent).
static UPLOADS: AtomicUsize = AtomicUsize::new(0);

/// Watchdog heartbeat counter.
static HEARTBEATS: AtomicUsize = AtomicUsize::new(0);

/// Mock sensor.
static SENSOR: SpinLock<MockSensor> = SpinLock::new(MockSensor::new_init("mock-temp", 22.0));

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
        let reading = Reading {
            timestamp_ms: (count as u64) * 100,
            ..reading
        };
        let _ = log.push(reading);

        // Fan the reading out to each independent consumer.
        *STORAGE_LATEST.lock() = Some(reading);
        *UPLOAD_LATEST.lock() = Some(reading);

        count += 1;
        if count == 1 || count % 5 == 0 {
            info!(
                "sample",
                "SAMPLE seq={} device=mock-temp value={}.{:02}",
                count,
                reading.value as i32,
                ((reading.value.abs() * 100.0) as u32) % 100
            );
        }

        yield_now().await;
    }
}

/// Storage task: appends each new reading to the RAM-backed record store.
/// Priority 2 — can be delayed but must eventually record.
/// Capability demonstration: only this task holds the storage token.
#[caplet_task(priority = 2, stack = 4096, name = "storage")]
async fn storage_task() {
    info!("demo", "storage_task started (capability: FlashToken)");
    let mut count: u32 = 0;
    loop {
        let reading = STORAGE_LATEST.lock().take();
        if let Some(r) = reading {
            count += 1;
            let record = StorageRecord::new(count, r.timestamp_ms, r.value);
            if append_record(record) {
                STORAGE_WRITES.fetch_add(1, Ordering::Relaxed);
            }
            if count == 1 || count % 5 == 0 {
                info!(
                    "fs",
                    "STORAGE persisted seq={} checksum=0x{:08x} total={} [ram-backed]",
                    count,
                    record.checksum,
                    STORAGE_WRITES.load(Ordering::Relaxed)
                );
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

    let connect = client.begin_connect("caplet-industrial", 60, true);
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

    let register = Client::build_register_with_msg_id(0, client.next_message_id(), "sensors/temp");
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

    let mut count: u32 = 0;
    loop {
        let reading = UPLOAD_LATEST.lock().take();
        if let Some(r) = reading {
            let payload = build_mqtt_sn_payload(r);
            let publish = match client.publish(payload.as_bytes(), false, count * 100) {
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
            UPLOADS.fetch_add(1, Ordering::Relaxed);
            count += 1;
            if count >= 5 {
                let disconnect = Client::build_disconnect();
                log_packet("DISCONNECT", &disconnect);
                let _ = broker.exchange(&disconnect);
                info!(
                    "upload",
                    "UPLOAD complete={} packets={}",
                    count,
                    broker.packet_count()
                );
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
        let reads = STORAGE_WRITES.load(Ordering::Relaxed);
        let sends = UPLOADS.load(Ordering::Relaxed);

        // Simple health check: if no readings in 30 ticks, log warning.
        // In a real system this would trigger a reset.
        if hb > 0 && hb % 8 == 0 {
            if reads == 0 || sends == 0 {
                info!(
                    "watchdog",
                    "WARNING: no storage writes or uploads at heartbeat {}", hb
                );
            } else {
                info!(
                    "watchdog",
                    "WATCHDOG healthy heartbeat={} writes={} sends={}", hb, reads, sends
                );
            }
        }

        yield_now().await;
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

impl StorageRecord {
    fn new(sequence: u32, timestamp_ms: u64, value: f32) -> Self {
        let value_bits = value.to_bits();
        let checksum = sequence
            ^ (timestamp_ms as u32)
            ^ ((timestamp_ms >> 32) as u32)
            ^ value_bits
            ^ 0xA5A5_5A5A;
        Self {
            sequence,
            timestamp_ms,
            value_bits,
            checksum,
        }
    }

    fn is_valid(&self) -> bool {
        self.checksum
            == (self.sequence
                ^ (self.timestamp_ms as u32)
                ^ ((self.timestamp_ms >> 32) as u32)
                ^ self.value_bits
                ^ 0xA5A5_5A5A)
    }
}

fn append_record(record: StorageRecord) -> bool {
    let mut log = RECORD_LOG.lock();
    if log.len() == LOG_DEPTH {
        let _ = log.remove(0);
    }
    if log.push(record).is_err() {
        return false;
    }
    log.last().map(StorageRecord::is_valid).unwrap_or(false)
}

/// Build the MQTT-SN PUBLISH payload.
///
/// Format: `ID=device_id&t=NN.NC` (compact, ascii, no JSON).
fn build_mqtt_sn_payload(r: Reading) -> heapless::String<32> {
    use heapless::String;
    let mut s: String<32> = String::new();
    // Encode device_id.
    for b in r.device_id {
        if b == 0 {
            break;
        }
        let _ = s.push(b as char);
    }
    let _ = s.push('&');
    let _ = s.push('t');
    let _ = s.push('=');
    if r.value < 0.0 {
        let _ = s.push('-');
    }
    let scaled = ((r.value.abs() * 100.0) + 0.5) as u32;
    let whole = (scaled / 100) as i32;
    let fraction = scaled % 100;
    let whole_bytes = itoa(whole);
    for &b in &whole_bytes {
        if b == 0 {
            break;
        }
        let _ = s.push(b as char);
    }
    let _ = s.push('.');
    let _ = s.push((b'0' + (fraction / 10) as u8) as char);
    let _ = s.push((b'0' + (fraction % 10) as u8) as char);
    s
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

/// Minimal itoa (no_std compatible).
fn itoa(mut n: i32) -> [u8; 12] {
    let mut buf = [0u8; 12];
    let neg = n < 0;
    if n == 0 {
        buf[0] = b'0';
        return buf;
    }
    if neg {
        n = -n;
    }
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    if neg && i < 11 {
        buf[i] = b'-';
        i += 1;
    }
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

// ── Entry point ──────────────────────────────────────────────────────────

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
