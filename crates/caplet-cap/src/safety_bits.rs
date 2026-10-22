//! Safety bits for capabilities

/// Safety bits control special behavior of capabilities
#[derive(Debug, Clone, Copy, Default)]
pub struct SafetyBits(pub u8);

/// Individual safety bit positions
const AUDIT_BIT: u8 = 1 << 0;
const INFECT_BIT: u8 = 1 << 1;
const PERSISTENT_BIT: u8 = 1 << 2;
const TRANSFERABLE_BIT: u8 = 1 << 3;
// M3 additions (bits 4-7).
const CROSS_NAMESPACE_BIT: u8 = 1 << 4;
const SHARE_PHYSICAL_BIT: u8 = 1 << 5;
const MMIO_DEVICE_BIT: u8 = 1 << 6;
const MODULE_BIT: u8 = 1 << 7;

impl SafetyBits {
    /// Check if cross-namespace is allowed.
    pub fn cross_namespace(&self) -> bool {
        (self.0 & CROSS_NAMESPACE_BIT) != 0
    }

    /// Set the cross-namespace bit.
    pub fn set_cross_namespace(&mut self, value: bool) {
        if value {
            self.0 |= CROSS_NAMESPACE_BIT;
        } else {
            self.0 &= !CROSS_NAMESPACE_BIT;
        }
    }

    /// Check if the capability points to shared physical memory.
    pub fn share_physical(&self) -> bool {
        (self.0 & SHARE_PHYSICAL_BIT) != 0
    }

    /// Set the shared-physical bit.
    pub fn set_share_physical(&mut self, value: bool) {
        if value {
            self.0 |= SHARE_PHYSICAL_BIT;
        } else {
            self.0 &= !SHARE_PHYSICAL_BIT;
        }
    }

    /// Check if this capability points to an MMIO device.
    pub fn mmio_device(&self) -> bool {
        (self.0 & MMIO_DEVICE_BIT) != 0
    }

    /// Set the MMIO-device bit.
    pub fn set_mmio_device(&mut self, value: bool) {
        if value {
            self.0 |= MMIO_DEVICE_BIT;
        } else {
            self.0 &= !MMIO_DEVICE_BIT;
        }
    }

    /// Check if this capability represents a loaded module.
    pub fn module(&self) -> bool {
        (self.0 & MODULE_BIT) != 0
    }

    /// Set the module bit.
    pub fn set_module(&mut self, value: bool) {
        if value {
            self.0 |= MODULE_BIT;
        } else {
            self.0 &= !MODULE_BIT;
        }
    }

    /// M3 attenuation: tighten the bits inherited from a parent.
    pub fn attenuate_m3(&self, parent: SafetyBits) -> Self {
        let mut result = *self;
        if !parent.cross_namespace() {
            result.0 &= !CROSS_NAMESPACE_BIT;
        }
        if !parent.share_physical() {
            result.0 &= !SHARE_PHYSICAL_BIT;
        }
        if !parent.mmio_device() {
            result.0 &= !MMIO_DEVICE_BIT;
        }
        if !parent.module() {
            result.0 &= !MODULE_BIT;
        }
        result
    }
}

impl PartialEq for SafetyBits {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for SafetyBits {}

impl SafetyBits {
    /// Create default safety bits (audit off, infect on, not persistent, transferable)
    pub fn default_for_user() -> Self {
        Self(INFECT_BIT | TRANSFERABLE_BIT)
    }

    /// Create safety bits for kernel capabilities
    pub fn kernel() -> Self {
        Self(0)
    }

    /// Create safety bits with all bits set
    pub fn all() -> Self {
        Self(AUDIT_BIT | INFECT_BIT | PERSISTENT_BIT | TRANSFERABLE_BIT)
    }

    /// Check if audit is enabled
    pub fn audit(&self) -> bool {
        (self.0 & AUDIT_BIT) != 0
    }

    /// Set audit bit
    pub fn set_audit(&mut self, value: bool) {
        if value {
            self.0 |= AUDIT_BIT;
        } else {
            self.0 &= !AUDIT_BIT;
        }
    }

    /// Check if infect (cascade revoke) is enabled
    pub fn infect(&self) -> bool {
        (self.0 & INFECT_BIT) != 0
    }

    /// Set infect bit
    pub fn set_infect(&mut self, value: bool) {
        if value {
            self.0 |= INFECT_BIT;
        } else {
            self.0 &= !INFECT_BIT;
        }
    }

    /// Check if persistent
    pub fn persistent(&self) -> bool {
        (self.0 & PERSISTENT_BIT) != 0
    }

    /// Set persistent bit
    pub fn set_persistent(&mut self, value: bool) {
        if value {
            self.0 |= PERSISTENT_BIT;
        } else {
            self.0 &= !PERSISTENT_BIT;
        }
    }

    /// Check if transferable
    pub fn transferable(&self) -> bool {
        (self.0 & TRANSFERABLE_BIT) != 0
    }

    /// Set transferable bit
    pub fn set_transferable(&mut self, value: bool) {
        if value {
            self.0 |= TRANSFERABLE_BIT;
        } else {
            self.0 &= !TRANSFERABLE_BIT;
        }
    }

    /// Attenuate these safety bits with parent bits
    pub fn attenuate(&self, parent: SafetyBits) -> Self {
        // Every safety bit is a restriction on the capability.  A derived
        // capability must therefore be a subset of its parent across both the
        // legacy bits (audit/infect/persistent/transferable) and the M3 bits.
        // Masking the complete word also covers future bit assignments.
        Self(self.0 & parent.0)
    }

    /// Get raw bits
    pub const fn bits(&self) -> u8 {
        self.0
    }

    /// Create from raw bits
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety_policy::SafetyPolicy;

    #[test]
    fn cross_namespace_bit_roundtrip() {
        let mut s = SafetyBits::default();
        assert!(!s.cross_namespace());
        s.set_cross_namespace(true);
        assert!(s.cross_namespace());
        s.set_cross_namespace(false);
        assert!(!s.cross_namespace());
    }

    #[test]
    fn share_physical_bit_roundtrip() {
        let mut s = SafetyBits::default();
        s.set_share_physical(true);
        assert!(s.share_physical());
    }

    #[test]
    fn mmio_device_bit_roundtrip() {
        let mut s = SafetyBits::default();
        s.set_mmio_device(true);
        assert!(s.mmio_device());
    }

    #[test]
    fn module_bit_roundtrip() {
        let mut s = SafetyBits::default();
        s.set_module(true);
        assert!(s.module());
    }

    #[test]
    fn m3_attenuation_tightens() {
        // First sub-case: parent allows every M3 bit, child should keep
        // all the bits the child originally had (which is all()).
        let mut parent = SafetyBits::default();
        parent.set_cross_namespace(true);
        parent.set_share_physical(true);
        parent.set_mmio_device(true);
        parent.set_module(true);
        let mut child = SafetyBits::default();
        child.set_cross_namespace(true);
        child.set_share_physical(true);
        child.set_mmio_device(true);
        child.set_module(true);
        let out = child.attenuate_m3(parent);
        assert!(out.cross_namespace());
        assert!(out.share_physical());
        assert!(out.mmio_device());
        assert!(out.module());

        // Parent disallows cross_namespace; child loses it.
        let mut parent2 = SafetyBits::default();
        parent2.set_share_physical(true);
        parent2.set_mmio_device(true);
        parent2.set_module(true);
        let mut child2 = SafetyBits::default();
        child2.set_cross_namespace(true);
        child2.set_share_physical(true);
        child2.set_mmio_device(true);
        child2.set_module(true);
        let out2 = child2.attenuate_m3(parent2);
        assert!(!out2.cross_namespace());
        assert!(out2.share_physical());
        assert!(out2.mmio_device());
        assert!(out2.module());
    }

    #[test]
    fn attenuation_rejects_legacy_bit_amplification() {
        let parent = SafetyBits::default();
        let child = SafetyBits::all();
        let out = child.attenuate(parent);
        assert_eq!(out.bits(), 0);
        assert!(!SafetyPolicy::validate_safety_bits(child, parent));
    }
}
