//! Endpoint capability derivation extensions (M3).
//!
//! An endpoint in CapletRT is a named IPC target. The capability
//! model lets a process hold a `Cap<Endpoint>` whose rights can be
//! attenuated. The M3 additions are:
//!
//! - **Namespace scoping**: a `Cap<Endpoint>` can be restricted to a
//!   single namespace; any publish into a different namespace is
//!   refused.
//! - **Rate limiting**: the cap carries a `max_msg_per_sec` value.
//!   The endpoint ref counts messages and refuses once the rate is
//!   exceeded.
//! - **One-shot**: a flag that causes the cap to self-revoke after
//!   one successful message.

use crate::error::{CapError, CapResult};
use crate::permissions::Permissions;
use crate::safety_bits::SafetyBits;

/// Rights attached to an endpoint capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointRights {
    /// Bitmask: bit 0 = publish, bit 1 = subscribe, bit 2 = grant.
    pub bits: u8,
    /// Namespace id this endpoint is scoped to. `None` means any
    /// namespace is allowed.
    pub namespace: Option<u32>,
    /// Maximum messages per second (0 = unlimited).
    pub max_msg_per_sec: u32,
    /// One-shot endpoint: consumed after a single successful use.
    pub one_shot: bool,
    /// Live counter of consumed messages in the current 1-second
    /// window. Kernel-managed.
    pub consumed_in_window: u32,
}

impl EndpointRights {
    /// Rights that let the holder publish and subscribe but not
    /// grant.
    pub const READ_WRITE: Self = Self {
        bits: 0b011,
        namespace: None,
        max_msg_per_sec: 0,
        one_shot: false,
        consumed_in_window: 0,
    };
    /// Rights that let the holder only publish.
    pub const WRITE_ONLY: Self = Self {
        bits: 0b001,
        namespace: None,
        max_msg_per_sec: 0,
        one_shot: false,
        consumed_in_window: 0,
    };
    /// Rights that let the holder only subscribe.
    pub const READ_ONLY: Self = Self {
        bits: 0b010,
        namespace: None,
        max_msg_per_sec: 0,
        one_shot: false,
        consumed_in_window: 0,
    };

    /// Construct a copy restricted to `ns`.
    pub fn scoped_to(&self, ns: u32) -> Self {
        let mut copy = *self;
        copy.namespace = Some(ns);
        copy
    }

    /// Construct a copy with a rate limit.
    pub fn with_rate_limit(&self, max: u32) -> Self {
        let mut copy = *self;
        copy.max_msg_per_sec = max;
        copy
    }

    /// Construct a copy that is one-shot.
    pub fn one_shot(&self) -> Self {
        let mut copy = *self;
        copy.one_shot = true;
        copy
    }

    /// Try to consume one message slot. Returns `Ok(())` if the slot
    /// was consumed, or `Err(CapError::PermissionDenied)` if the
    /// rate limit was hit.
    pub fn try_consume(&mut self) -> CapResult<()> {
        if self.max_msg_per_sec > 0 && self.consumed_in_window >= self.max_msg_per_sec {
            return Err(CapError::Permission);
        }
        self.consumed_in_window = self.consumed_in_window.saturating_add(1);
        Ok(())
    }

    /// Reset the rate-limit window (call once per second by the
    /// kernel).
    pub fn reset_window(&mut self) {
        self.consumed_in_window = 0;
    }

    /// Returns `true` if the endpoint allows publishing.
    pub fn may_publish(&self) -> bool {
        (self.bits & 0b001) != 0
    }

    /// Returns `true` if the endpoint allows subscribing.
    pub fn may_subscribe(&self) -> bool {
        (self.bits & 0b010) != 0
    }

    /// Returns `true` if the endpoint allows granting to other caps.
    pub fn may_grant(&self) -> bool {
        (self.bits & 0b100) != 0
    }
}

impl Default for EndpointRights {
    fn default() -> Self {
        Self::READ_WRITE
    }
}

/// Bundle of endpoint rights + the standard safety bits.
#[derive(Debug, Clone, Copy)]
pub struct EndpointCapability {
    /// Endpoint-specific rights.
    pub rights: EndpointRights,
    /// Safety bits.
    pub safety: SafetyBits,
    /// Permissions attached to the cap.
    pub perms: Permissions,
}

impl Default for EndpointCapability {
    fn default() -> Self {
        Self {
            rights: EndpointRights::default(),
            safety: SafetyBits::default(),
            perms: Permissions::empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rights_allow_publish_and_subscribe() {
        let r = EndpointRights::default();
        assert!(r.may_publish());
        assert!(r.may_subscribe());
        assert!(!r.may_grant());
    }

    #[test]
    fn write_only_blocks_subscribe() {
        let r = EndpointRights::WRITE_ONLY;
        assert!(r.may_publish());
        assert!(!r.may_subscribe());
    }

    #[test]
    fn rate_limit_enforced() {
        let mut r = EndpointRights::READ_WRITE.with_rate_limit(2);
        assert!(r.try_consume().is_ok());
        assert!(r.try_consume().is_ok());
        assert!(r.try_consume().is_err());
        r.reset_window();
        assert!(r.try_consume().is_ok());
    }

    #[test]
    fn scoped_to_copies() {
        let r = EndpointRights::READ_WRITE.scoped_to(42);
        assert_eq!(r.namespace, Some(42));
        // Original is unchanged.
        assert_eq!(EndpointRights::READ_WRITE.namespace, None);
    }

    #[test]
    fn one_shot_marks() {
        let r = EndpointRights::READ_WRITE.one_shot();
        assert!(r.one_shot);
    }
}
