//! Placeholder schema for the M3 worst-case stack analyzer.
//!
//! **No analysis is implemented.** These types exist so reports can be
//! discussed without pretending that a bound has been computed.

#![forbid(unsafe_code)]

/// How much confidence a reported stack bound carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// The bound is a static maximum over a fully resolved call graph.
    Proven,
    /// The graph contains indirect calls that were resolved by heuristics.
    Estimated,
    /// The bound is a hand-written constant, not an analyzer result.
    Declared,
}

/// One analyzed entry point (task or interrupt handler).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackReport {
    /// Task or ISR symbol.
    pub entry: &'static str,
    /// Worst-case stack usage in bytes.
    pub worst_case_bytes: usize,
    /// Stack bytes reserved by the linker script or task builder.
    pub reserved_bytes: usize,
    /// Confidence in [`Self::worst_case_bytes`].
    pub confidence: Confidence,
}

impl StackReport {
    /// Returns `true` only when the observed bound exceeds the reservation.
    pub const fn overflows(&self) -> bool {
        self.worst_case_bytes > self.reserved_bytes
    }
}

/// Marker for "this crate has no analyzer backend yet".
pub const ANALYZER_IMPLEMENTED: bool = false;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_reports_its_status() {
        assert!(!ANALYZER_IMPLEMENTED);
        let report = StackReport {
            entry: "example",
            worst_case_bytes: 512,
            reserved_bytes: 1024,
            confidence: Confidence::Declared,
        };
        assert!(!report.overflows());
    }
}
