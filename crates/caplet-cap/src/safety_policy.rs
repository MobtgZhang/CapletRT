//! Safety policy enforcement

use crate::permissions::Permissions;
use crate::safety_bits::SafetyBits;

/// Safety policy for capability derivation
pub struct SafetyPolicy;

impl SafetyPolicy {
    /// Validate that child safety bits are a valid attenuation of parent
    pub fn validate_safety_bits(child: SafetyBits, parent: SafetyBits) -> bool {
        let attenuated = child.attenuate(parent);
        attenuated.0 == child.0
    }

    /// Validate that child permissions are a valid attenuation of parent
    pub fn validate_permissions(child: Permissions, parent: Permissions) -> bool {
        (parent.0 & child.0) == child.0
    }

    /// Validate entire capability derivation
    pub fn validate_derivation(
        child_safety: SafetyBits,
        child_perms: Permissions,
        parent_safety: SafetyBits,
        parent_perms: Permissions,
    ) -> DerivationValidationResult {
        let safety_ok = Self::validate_safety_bits(child_safety, parent_safety);
        let perms_ok = Self::validate_permissions(child_perms, parent_perms);

        if safety_ok && perms_ok {
            DerivationValidationResult::Valid
        } else if !safety_ok && !perms_ok {
            DerivationValidationResult::InvalidBoth
        } else if !safety_ok {
            DerivationValidationResult::InvalidSafetyBits
        } else {
            DerivationValidationResult::InvalidPermissions
        }
    }

    /// Get the restrictive combination of two safety bit sets
    pub fn restrictive_safety(child: SafetyBits, parent: SafetyBits) -> SafetyBits {
        child.attenuate(parent)
    }

    /// Get the restrictive combination of two permission sets
    pub fn restrictive_permissions(child: Permissions, parent: Permissions) -> Permissions {
        child & parent
    }
}

/// Result of derivation validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivationValidationResult {
    /// Derivation is valid
    Valid,
    /// Both safety bits and permissions are invalid
    InvalidBoth,
    /// Only safety bits are invalid
    InvalidSafetyBits,
    /// Only permissions are invalid
    InvalidPermissions,
}

impl DerivationValidationResult {
    /// Check if this represents a valid derivation
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }

    /// Check if safety bits are valid
    pub fn has_valid_safety(&self) -> bool {
        !matches!(self, Self::InvalidBoth | Self::InvalidSafetyBits)
    }

    /// Check if permissions are valid
    pub fn has_valid_permissions(&self) -> bool {
        !matches!(self, Self::InvalidBoth | Self::InvalidPermissions)
    }
}
