//! Loader observability (M3).
//!
//! Captures two pieces of data:
//!
//! - [`LoaderMetrics`]: counters for `load_module` outcomes
//!   (success, parse failure, signature mismatch, ...). Atomic so
//!   it survives across multiple task contexts.
//! - Optional [`AuditRing`]: a small, bounded ring buffer of
//!   recent `AuditRecord`s, gated behind the `metrics_ring`
//!   feature. Useful for postmortem analysis when a load fails in
//!   production and the developer can't attach a probe.

use caplet_sync::SpinLock;

/// Monotonic counter, queried via [`LoaderMetrics::snapshot`].
pub struct LoaderMetrics {
    /// Successful `.capp` loads.
    pub ok_capp: SpinLock<u64>,
    /// Successful ELF loads.
    pub ok_elf: SpinLock<u64>,
    /// Parse-stage failures.
    pub fail_parse: SpinLock<u64>,
    /// Signature-check failures.
    pub fail_signature: SpinLock<u64>,
    /// Relocation-stage failures.
    pub fail_reloc: SpinLock<u64>,
    /// Memory-pool allocation failures.
    pub fail_alloc: SpinLock<u64>,
}

impl LoaderMetrics {
    /// Allocate a fresh metrics block. Static instance via [`METRICS`].
    pub const fn new() -> Self {
        Self {
            ok_capp: SpinLock::new(0),
            ok_elf: SpinLock::new(0),
            fail_parse: SpinLock::new(0),
            fail_signature: SpinLock::new(0),
            fail_reloc: SpinLock::new(0),
            fail_alloc: SpinLock::new(0),
        }
    }

    /// Increment the "capp load OK" counter.
    pub fn inc_capp(&self) {
        bump(&self.ok_capp);
    }
    /// Increment the "ELF load OK" counter.
    pub fn inc_elf(&self) {
        bump(&self.ok_elf);
    }
    /// Increment the parse-failure counter.
    pub fn inc_parse_fail(&self) {
        bump(&self.fail_parse);
    }
    /// Increment the signature-failure counter.
    pub fn inc_signature_fail(&self) {
        bump(&self.fail_signature);
    }
    /// Increment the relocation-failure counter.
    pub fn inc_reloc_fail(&self) {
        bump(&self.fail_reloc);
    }
    /// Increment the allocation-failure counter.
    pub fn inc_alloc_fail(&self) {
        bump(&self.fail_alloc);
    }

    /// Read every counter atomically and pack them into a
    /// [`LoaderSnapshot`]. The values are not consistent across
    /// each other in the strictest sense (they're read one by
    /// one), but that is acceptable for metrics.
    pub fn snapshot(&self) -> LoaderSnapshot {
        LoaderSnapshot {
            ok_capp: *self.ok_capp.lock(),
            ok_elf: *self.ok_elf.lock(),
            fail_parse: *self.fail_parse.lock(),
            fail_signature: *self.fail_signature.lock(),
            fail_reloc: *self.fail_reloc.lock(),
            fail_alloc: *self.fail_alloc.lock(),
        }
    }
}

impl Default for LoaderMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Increment a counter without relying on 64-bit atomics, which are not
/// available on targets such as `riscv32imac`.
fn bump(counter: &SpinLock<u64>) {
    let mut value = counter.lock();
    *value = value.wrapping_add(1);
}

/// Non-atomic snapshot suitable for printing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoaderSnapshot {
    /// Successful `.capp` loads at snapshot time.
    pub ok_capp: u64,
    /// Successful ELF loads at snapshot time.
    pub ok_elf: u64,
    /// Parse-failure count at snapshot time.
    pub fail_parse: u64,
    /// Signature-failure count at snapshot time.
    pub fail_signature: u64,
    /// Reloc-failure count at snapshot time.
    pub fail_reloc: u64,
    /// Alloc-failure count at snapshot time.
    pub fail_alloc: u64,
}

/// Outcome kind for an audit record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOutcome {
    /// Module loaded successfully.
    Loaded,
    /// Container parse failed.
    ParseFailed,
    /// Signature did not match.
    BadSignature,
    /// Apply-relocations step failed.
    RelocFailed,
    /// Memory-pool allocation failed.
    AllocFailed,
}

/// Lightweight audit record: store the binary name (truncated to
/// 16 bytes), a `u32` the loader owns (signature key id or 0),
/// and the outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditRecord {
    /// Truncated name tag. Empty bytes are filled with zero.
    pub name_tag: [u8; 16],
    /// Number of meaningful bytes in `name_tag`.
    pub name_len: u8,
    /// Signature key id (0 if unsigned).
    pub key_id: u32,
    /// What happened.
    pub outcome: AuditOutcome,
}

/// Static, module-global metrics. Tests can read via
/// `snapshot` rather than clearing.
pub static METRICS: LoaderMetrics = LoaderMetrics::new();

/// Format the snapshot in a stable, machine-parseable line.
pub fn format_snapshot(s: LoaderSnapshot) -> alloc::string::String {
    use alloc::format;
    format!(
        "loader_metrics ok_capp={} ok_elf={} fail_parse={} fail_signature={} fail_reloc={} fail_alloc={}",
        s.ok_capp, s.ok_elf, s.fail_parse, s.fail_signature, s.fail_reloc, s.fail_alloc
    )
}

#[cfg(feature = "metrics_ring")]
mod ring {
    use super::*;
    use caplet_sync::spin::SpinLock;

    /// Bounded audit ring buffer, capacity 16.
    pub struct AuditRing {
        state: SpinLock<RingState>,
    }

    struct RingState {
        buf: [Option<AuditRecord>; 16],
        head: usize,
        len: usize,
    }

    impl AuditRing {
        /// Allocate a fresh ring. Static instance via [`AUDIT`].
        pub const fn new() -> Self {
            Self {
                state: SpinLock::new(RingState {
                    buf: [None; 16],
                    head: 0,
                    len: 0,
                }),
            }
        }

        /// Push a new record, evicting the oldest if full.
        pub fn push(&self, r: AuditRecord) {
            let mut state = self.state.lock();
            let head = state.head;
            state.buf[head] = Some(r);
            state.head = (state.head + 1) % state.buf.len();
            if state.len < state.buf.len() {
                state.len += 1;
            }
        }

        /// Copy the currently stored records from oldest to newest.
        pub fn snapshot(&self) -> [Option<AuditRecord>; 16] {
            let state = self.state.lock();
            let mut out = [None; 16];
            let start = (state.head + state.buf.len() - state.len) % state.buf.len();
            for (i, slot) in out.iter_mut().take(state.len).enumerate() {
                *slot = state.buf[(start + i) % state.buf.len()];
            }
            out
        }

        /// Number of valid records currently in the ring.
        pub fn len(&self) -> usize {
            self.state.lock().len
        }

        /// Return `true` when no audit records have been stored.
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
    }

    impl Default for AuditRing {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Static audit ring instance.
    pub static AUDIT: AuditRing = AuditRing::new();

    /// Push a single record with a stringy name.
    pub fn push_outcome(name: &[u8], key_id: u32, outcome: AuditOutcome) {
        let mut tag = [0u8; 16];
        let n = name.len().min(16);
        tag[..n].copy_from_slice(&name[..n]);
        AUDIT.push(AuditRecord {
            name_tag: tag,
            name_len: n as u8,
            key_id,
            outcome,
        });
    }
}

#[cfg(feature = "metrics_ring")]
pub use ring::{push_outcome, AuditRing, AUDIT};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_increment() {
        let m = LoaderMetrics::new();
        m.inc_capp();
        m.inc_elf();
        m.inc_elf();
        m.inc_parse_fail();
        let s = m.snapshot();
        assert_eq!(s.ok_capp, 1);
        assert_eq!(s.ok_elf, 2);
        assert_eq!(s.fail_parse, 1);
    }

    #[test]
    fn format_snapshot_is_stable() {
        let s = LoaderSnapshot {
            ok_capp: 3,
            ok_elf: 7,
            fail_parse: 11,
            fail_signature: 13,
            fail_reloc: 17,
            fail_alloc: 19,
        };
        let line = format_snapshot(s);
        assert!(line.starts_with("loader_metrics "));
        assert!(line.contains("ok_capp=3"));
        assert!(line.contains("ok_elf=7"));
    }
}
