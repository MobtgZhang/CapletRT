//! Capability error types

/// Errors that can occur during capability operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapError {
    /// Capability not found
    NotFound,
    /// Permission denied
    Permission,
    /// Invalid operation
    Invalid,
    /// Out of memory
    OutOfMemory,
    /// Out of slots
    OutOfSlots,
    /// Capability revoked
    Revoked,
    /// Capability expired
    Expired,
    /// Invalid derivation
    InvalidDerivation,
    /// Revocation failed
    RevocationFailed,
    /// CSpace full
    CSpaceFull,
    /// The underlying resource is temporarily unavailable
    Unavailable,
    /// An operation exceeded its deadline
    Timeout,
    /// The underlying hardware reported a failure
    Hardware,
}

impl CapError {
    /// Returns a short description of the error
    pub const fn as_str(&self) -> &'static str {
        match self {
            CapError::NotFound => "Capability not found",
            CapError::Permission => "Permission denied",
            CapError::Invalid => "Invalid operation",
            CapError::OutOfMemory => "Out of memory",
            CapError::OutOfSlots => "Out of capability slots",
            CapError::Revoked => "Capability has been revoked",
            CapError::Expired => "Capability has expired",
            CapError::InvalidDerivation => "Invalid capability derivation",
            CapError::RevocationFailed => "Capability revocation failed",
            CapError::CSpaceFull => "CSpace is full",
            CapError::Unavailable => "Resource unavailable",
            CapError::Timeout => "Operation timed out",
            CapError::Hardware => "Hardware failure",
        }
    }
}

impl core::fmt::Display for CapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Result type for capability operations
pub type CapResult<T> = Result<T, CapError>;
