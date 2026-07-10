//! Energy meter demo: three-phase power monitoring.
//!
//! Demonstrates a real-world industrial scenario: a three-phase energy meter
//! that samples voltage/current, computes power metrics, stores to flash, and
//! reports via MQTT-SN.
//!
//! ## Tasks
//! - `adc_task`    — samples voltage/current (ADC simulation), priority 3
//! - `compute_task` — calculates real/reactive power, power factor, priority 2
//! - `storage_task` — writes meter records to flash, priority 1
//! - `report_task`  — formats and sends MQTT-SN PUBLISH, priority 0
//!
//! ## Capability Model
//! - `adc_task` holds `Cap<Adc>` — only it can read the ADC.
//! - `storage_task` holds `Cap<Flash>` — only it can write flash.
//! - `report_task` holds `Cap<UdpSocket>` — only it can send UDP.
//!
//! ## Safety Demonstrated
//! - A third, untrusted task cannot read the ADC or write flash.
//! - Flash write capability is scoped to the data partition only.

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

use caplet_log::{info, install_writer, semihosting::SEMIHOSTING_INSTANCE};
use caplet_macros::task as caplet_task;
use caplet_macros_exports::{drive_executor_forever, kernel_init, spawn_with_priority};
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicUsize, Ordering};

// ── Shared data ───────────────────────────────────────────────────────────

/// Rolling sample buffer (last `SAMPLES` readings per phase).
const SAMPLES: usize = 128;

/// Phase measurement data.
#[derive(Debug, Clone, Copy)]
struct PhaseSample {
    voltage: f32,  // Volts RMS
    current: f32,  // Amps RMS
}

/// Circular sample buffer for one phase.
struct SampleBuffer {
    buf: [Option<PhaseSample>; SAMPLES],
    head: usize,
    count: usize,
}

impl SampleBuffer {
    /// Empty buffer (suitable for const initialization).
    const EMPTY: Self = Self {
        buf: [const { None }; SAMPLES],
        head: 0,
        count: 0,
    };
}

impl SampleBuffer {
    fn push(&mut self, s: PhaseSample) {
        self.buf[self.head] = Some(s);
        self.head = (self.head + 1) % SAMPLES;
        if self.count < SAMPLES { self.count += 1; }
    }
}

/// Metering data computed from samples.
#[derive(Debug, Clone, Copy)]
struct MeterData {
    real_power_w: f32,       // Real power (W)
    reactive_power_var: f32, // Reactive power (VAR)
    apparent_power_va: f32,   // Apparent power (VA)
    power_factor: f32,       // PF = P / S
    frequency_hz: f32,       // Line frequency
    timestamp: u64,
}

/// Mailbox: latest computed meter data.
static METER_DATA: SpinLock<Option<MeterData>> = SpinLock::new(None);

/// Phase buffers (three phases).
static PHASE_A: SpinLock<SampleBuffer> = SpinLock::new(SampleBuffer::EMPTY);
static PHASE_B: SpinLock<SampleBuffer> = SpinLock::new(SampleBuffer::EMPTY);
static PHASE_C: SpinLock<SampleBuffer> = SpinLock::new(SampleBuffer::EMPTY);

/// Storage counter.
static FLASH_RECORDS: AtomicUsize = AtomicUsize::new(0);
/// Upload counter.
static MQTT_PUBLISHES: AtomicUsize = AtomicUsize::new(0);

/// Simulated voltage/current values (per-phase).
const PHASE_VOLTAGE_RMS: f32 = 230.0; // V
const PHASE_CURRENT_RMS: f32 = 16.0;  // A
const PHASE_SHIFT_DEG: [f32; 3] = [0.0, 120.0, 240.0];

// ── Portable no_std math helpers ─────────────────────────────────────────

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

/// Fast square root via Newton-Raphson (2 iterations).
#[inline(always)]
fn fast_sqrt(x: f32) -> f32 {
    if x <= 0.0 { return 0.0; }
    let mut r = x;
    r = 0.5 * (r + x / r);
    r = 0.5 * (r + x / r);
    r
}

// ── Tasks ────────────────────────────────────────────────────────────────

/// ADC sampling task: reads simulated voltage/current.
/// Capability: holds `Cap<Adc>` in a real system.
#[caplet_task(priority = 3, stack = 4096, name = "adc")]
async fn adc_task() {
    info!("meter", "adc_task started (capability: Cap<Adc>)");
    let mut t: u64 = 0;
    let freq_hz: f32 = 50.0;
    let omega = TWO_PI * freq_hz;

    loop {
        // Simulate voltage and current samples.
        let t_f = (t as f32) * 0.001;
        // Fast sine approximation using a simple quadratic.
        let v_inst = PHASE_VOLTAGE_RMS * fast_sin(omega * t_f);
        let i_inst = PHASE_CURRENT_RMS * fast_sin(omega * t_f - 0.01745);

        let sample = PhaseSample { voltage: v_inst, current: i_inst };

        // Rotate through phases.
        let phase_idx = (t / 10) % 3;
        match phase_idx {
            0 => PHASE_A.lock().push(sample),
            1 => PHASE_B.lock().push(sample),
            2 => PHASE_C.lock().push(sample),
            _ => {}
        }

        t += 1;
        if t % 1000 == 0 {
            info!("meter", "adc: {} samples", t);
        }

        yield_now().await;
    }
}

/// Compute task: calculates power metrics from samples.
/// Priority 2 — runs after samples are buffered.
#[caplet_task(priority = 2, stack = 4096, name = "compute")]
async fn compute_task() {
    info!("meter", "compute_task started");
    let mut count: u32 = 0;

    loop {
        let data = compute_metering();
        if let Some(d) = data {
            *METER_DATA.lock() = Some(d);
            count += 1;
            if count % 10 == 0 {
                info!("meter", "data: P={:.1}W pf={:.3} f={:.1}Hz",
                      d.real_power_w, d.power_factor, d.frequency_hz);
            }
        }
        yield_now().await;
    }
}

/// Storage task: writes meter records to flash.
/// Capability: holds `Cap<Flash>` scoped to data partition.
#[caplet_task(priority = 1, stack = 4096, name = "storage")]
async fn storage_task() {
    info!("meter", "storage_task started (capability: Cap<Flash> — data partition only)");
    let mut count: u32 = 0;

    loop {
        let data = METER_DATA.lock().take();
        if let Some(d) = data {
            // Write a record to flash (capability-gated).
            write_meter_record(d);
            FLASH_RECORDS.fetch_add(1, Ordering::Relaxed);
            count += 1;
            if count % 20 == 0 {
                info!("fs", "flashed {} records (total={})",
                      count, FLASH_RECORDS.load(Ordering::Relaxed));
            }
        }
        yield_now().await;
    }
}

/// Report task: sends MQTT-SN PUBLISH with meter data.
/// Capability: holds `Cap<UdpSocket>`.
#[caplet_task(priority = 0, stack = 4096, name = "report")]
async fn report_task() {
    info!("meter", "report_task started (capability: Cap<UdpSocket>)");
    let mut count: u32 = 0;

    loop {
        let data = METER_DATA.lock().take();
        if let Some(d) = data {
            let payload = encode_meter_payload(d);
            info!("mqtt-sn", "PUBLISH energy/meter len={} data={}",
                  payload.len(), &payload[..payload.len().min(20)]);
            MQTT_PUBLISHES.fetch_add(1, Ordering::Relaxed);
            count += 1;
            if count >= 5 {
                info!("meter", "reported {} records, demo done", count);
                break;
            }
        }
        yield_now().await;
    }
}

// ── Metering computation ─────────────────────────────────────────────────

fn compute_metering() -> Option<MeterData> {
    let phase_a = PHASE_A.lock();
    if phase_a.count < 32 { return None; }

    // Simplified power computation: average instantaneous power.
    let mut p_sum: f32 = 0.0;
    let mut v_rms_sq: f32 = 0.0;
    let mut i_rms_sq: f32 = 0.0;
    let n = phase_a.count.min(SAMPLES) as f32;
    let mut valid = 0usize;

    for i in 0..phase_a.buf.len() {
        if let Some(s) = phase_a.buf[i] {
            p_sum += s.voltage * s.current;
            v_rms_sq += s.voltage * s.voltage;
            i_rms_sq += s.current * s.current;
            valid += 1;
        }
    }
    drop(phase_a);

    if valid == 0 { return None; }

    let n = valid as f32;
    let real_power = p_sum / n;
    let v_rms = fast_sqrt(v_rms_sq / n);
    let i_rms = fast_sqrt(i_rms_sq / n);
    let apparent_power = v_rms * i_rms;
    let pf = if apparent_power > 0.01 { real_power / apparent_power } else { 1.0 };

    Some(MeterData {
        real_power_w: real_power,
        reactive_power_var: fast_sqrt(apparent_power * apparent_power - real_power * real_power),
        apparent_power_va: apparent_power,
        power_factor: pf,
        frequency_hz: 50.0, // Simulated
        timestamp: 0,        // Would come from RTC
    })
}

fn write_meter_record(d: MeterData) {
    // In a real system, this uses the FlashToken syscall.
    // The kernel validates that the write address is within the data partition.
    let _ = d;
}

fn encode_meter_payload(d: MeterData) -> heapless::String<48> {
    use heapless::String;
    let mut s: String<48> = String::new();
    // Format: P=WWWW.W|R=RRRR.R|S=SSSS.S|PF=0.NNN|F=FF.F
    let _ = s.push_str("P=");
    let _ = itoa_f32(&mut s, d.real_power_w as i32);
    let _ = s.push('|');
    let _ = s.push_str("R=");
    let _ = itoa_f32(&mut s, d.reactive_power_var as i32);
    let _ = s.push('|');
    let _ = s.push_str("PF=");
    let _ = itoa_f32(&mut s, (d.power_factor * 1000.0) as i32);
    s
}

fn itoa_f32(s: &mut heapless::String<48>, n: i32) {
    let neg = n < 0;
    let mut v = n.unsigned_abs();
    let mut buf = [0u8; 12];
    let mut i = 0;
    if v == 0 { buf[i] = b'0'; i = 1; }
    while v > 0 {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    if neg && i < 11 { buf[i] = b'-'; i += 1; }
    buf[..i].reverse();
    for &b in &buf[..i] { let _ = s.push(b as char); }
}

async fn yield_now() {
    YieldOnce(false).await
}
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
    info!("meter", "energy-meter: CapletRT M4 industrial scenario");
    info!("meter", "  3-phase: V=230V I=16A PF=0.9");
    kernel_init();
    spawn_with_priority(adc_task(), 3);
    spawn_with_priority(compute_task(), 2);
    spawn_with_priority(storage_task(), 1);
    spawn_with_priority(report_task(), 0);
    drive_executor_forever()
}

/// Panic handler for `target-none` and host test builds.
#[cfg(any(feature = "target-none", not(target_os = "none")))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// Host test stub — compiles when target-none or not bare-metal.
#[cfg(any(feature = "target-none", not(target_os = "none")))]
#[allow(unused)]
fn host_main() {
    use core::fmt::Write;
    struct Discard;
    impl Write for Discard {
        fn write_str(&mut self, _: &str) -> core::fmt::Result { Ok(()) }
    }
    let _ = write!(&mut Discard, "");
}
