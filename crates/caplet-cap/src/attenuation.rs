//! Attenuation utilities

use crate::permissions::Permissions;

/// Attenuation options when deriving a capability
#[derive(Debug, Clone, Copy, Default)]
pub struct Attenuation {
    /// New permissions after attenuation
    new_permissions: Permissions,
    /// Whether to enforce strict attenuation
    strict: bool,
}

impl Attenuation {
    /// Create a new attenuation with specified permissions
    pub const fn new(permissions: Permissions) -> Self {
        Self {
            new_permissions: permissions,
            strict: true,
        }
    }

    /// Create an attenuation that removes write permission
    pub fn read_only() -> Self {
        Self::new(Permissions::from(1)) // Only READ
    }

    /// Create an attenuation that removes read permission
    pub fn write_only() -> Self {
        Self::new(Permissions::from(2)) // Only WRITE
    }

    /// Set strict mode
    pub fn set_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Apply attenuation to parent permissions
    pub fn apply(&self, parent: Permissions) -> Result<Permissions, crate::error::CapError> {
        use crate::permissions::attenuate_permissions;
        use crate::permissions::is_valid_attenuation;

        if !is_valid_attenuation(parent, self.new_permissions) {
            return Err(crate::error::CapError::InvalidDerivation);
        }

        let attenuated = attenuate_permissions(parent, self.new_permissions);

        if self.strict && attenuated == parent {
            return Err(crate::error::CapError::InvalidDerivation);
        }

        Ok(attenuated)
    }

    /// Get the attenuated permissions
    pub fn permissions(&self) -> Permissions {
        self.new_permissions
    }
}

/// Options for capability derivation
#[derive(Debug, Clone, Copy, Default)]
pub struct DerivationOptions {
    /// Permission attenuation
    attenuation: Option<Attenuation>,
    /// Time-to-live in milliseconds (0 = no expiry)
    ttl_ms: u64,
    /// Maximum number of uses (0 = unlimited)
    max_uses: u32,
    /// Whether to enable audit
    enable_audit: bool,
    /// Whether this is a one-time capability
    one_time: bool,
}

impl DerivationOptions {
    /// Create new derivation options
    pub const fn new() -> Self {
        Self {
            attenuation: None,
            ttl_ms: 0,
            max_uses: 0,
            enable_audit: false,
            one_time: false,
        }
    }

    /// Set time-to-live
    pub const fn with_ttl(mut self, ms: u64) -> Self {
        self.ttl_ms = ms;
        self
    }

    /// Set maximum uses
    pub const fn with_max_uses(mut self, count: u32) -> Self {
        self.max_uses = count;
        self
    }

    /// Enable audit
    pub const fn with_audit(mut self) -> Self {
        self.enable_audit = true;
        self
    }

    /// Set as one-time capability
    pub const fn one_time(mut self) -> Self {
        self.one_time = true;
        self.max_uses = 1;
        self
    }

    /// Set attenuation
    pub fn attenuate(mut self, attenuation: Attenuation) -> Self {
        self.attenuation = Some(attenuation);
        self
    }

    /// Get TTL in milliseconds
    pub const fn ttl_ms(&self) -> u64 {
        self.ttl_ms
    }

    /// Get max uses
    pub const fn max_uses(&self) -> u32 {
        self.max_uses
    }

    /// Check if audit is enabled
    pub const fn is_audit_enabled(&self) -> bool {
        self.enable_audit
    }

    /// Check if this is one-time
    pub const fn is_one_time(&self) -> bool {
        self.one_time
    }
}