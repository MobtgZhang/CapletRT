//! Placeholder data model for the HAL v0.1 -> v0.2 migration tool.
//!
//! **No source rewriting is implemented.** The crate exists so the M3
//! migration flow has a checked-in home and a stable vocabulary. Callers
//! must not treat [`V0_1_TO_V0_2`] as an executable migration.

#![forbid(unsafe_code)]

/// One manual migration action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationStep {
    /// Symbol or manifest field affected by the change.
    pub target: &'static str,
    /// Human-readable action required for the v0.2 HAL.
    pub action: &'static str,
    /// Whether the step is mechanical or needs a design decision.
    pub mechanical: bool,
}

/// The reviewed v0.1 -> v0.2 migration checklist.
pub const V0_1_TO_V0_2: &[MigrationStep] = &[
    MigrationStep {
        target: "caplet-hal::uart::Uart",
        action: "unify the write error type with the v0.2 UartError",
        mechanical: true,
    },
    MigrationStep {
        target: "caplet-hal::timer::Timer",
        action: "split the monotonic tick source from the one-shot delay source",
        mechanical: false,
    },
    MigrationStep {
        target: "caplet-hal::interrupt::Interrupt",
        action: "register handlers through the capability-checked API",
        mechanical: false,
    },
    MigrationStep {
        target: "caplet-hal::phy::Phy",
        action: "report LinkState instead of a boolean link flag",
        mechanical: true,
    },
    MigrationStep {
        target: "Cargo.toml",
        action: "raise the caplet-hal requirement from 0.1 to 0.2",
        mechanical: true,
    },
];

/// Number of manual steps in the v0.1 -> v0.2 checklist.
pub const fn step_count() -> usize {
    V0_1_TO_V0_2.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checklist_is_not_empty() {
        assert_eq!(step_count(), V0_1_TO_V0_2.len());
        assert!(step_count() >= 4);
    }
}
