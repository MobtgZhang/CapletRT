//! Log level definitions

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Trace level - most verbose
    Trace = 0,
    /// Debug level
    Debug = 1,
    /// Info level
    Info = 2,
    /// Warning level
    Warn = 3,
    /// Error level
    Error = 4,
    /// No logging
    Off = 5,
}

impl Level {
    /// Get the level from a string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "trace" => Some(Level::Trace),
            "debug" => Some(Level::Debug),
            "info" => Some(Level::Info),
            "warn" | "warning" => Some(Level::Warn),
            "error" => Some(Level::Error),
            "off" => Some(Level::Off),
            _ => None,
        }
    }

    /// Decode a level from its raw discriminant (0..=5).
    ///
    /// Out-of-range values clamp to [`Level::Off`] so the loader can
    /// never panic on corrupt memory.
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Level::Trace,
            1 => Level::Debug,
            2 => Level::Info,
            3 => Level::Warn,
            4 => Level::Error,
            _ => Level::Off,
        }
    }

    /// Get the level name
    pub fn name(&self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Off => "OFF",
        }
    }
}

impl Default for Level {
    fn default() -> Self {
        Level::Info
    }
}

impl core::fmt::Display for Level {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.name())
    }
}
