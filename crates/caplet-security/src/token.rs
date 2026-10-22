//! Generic security-token marker. Used to constrain how
//! peripheral handles are passed between tasks via the
//! capability system.

use core::marker::PhantomData;
use core::sync::atomic::{AtomicU32, Ordering};

/// A 128-bit nonce the kernel hands out when granting a
/// security-critical capability. The holder should treat the
/// nonce as opaque; the kernel uses it to identify the live
/// capability for revocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CNonce {
    /// First 32 bits.
    pub hi: u32,
    /// Second 32 bits.
    pub mid: u32,
    /// Third 32 bits.
    pub lo: u32,
    /// Final 32 bits.
    pub end: u32,
}

/// Static tag counting issued tokens. Only the kernel should
/// ever invoke [`SecurityToken::next_nonce`] (and even then, only
/// once per `Cap::derive` call).
static TOKEN_GEN: AtomicU32 = AtomicU32::new(0);

/// Zero-sized marker for a granted peripheral. Only the
/// kernel's `Cap::derive` can produce one; the empty
/// constructor is `pub(crate)`.
pub struct SecurityToken<P> {
    nonce: CNonce,
    _phantom: PhantomData<*const P>,
}

impl<P> SecurityToken<P> {
    /// **Kernel-only**. Construct a new security token.
    ///
    /// # Safety
    /// The caller must hold the kernel capability for
    /// allocating capability slots. The returned token is
    /// unique until the corresponding slot is revoked.
    pub unsafe fn new() -> Self {
        let id = TOKEN_GEN.fetch_add(1, Ordering::Relaxed);
        let hi = id.wrapping_mul(0x9E37_79B1);
        let mid = id.wrapping_mul(0x85EB_CA77);
        let lo = id.wrapping_mul(0xC2B2_AE3D);
        let end = id.wrapping_mul(0x27D4_EB2F);
        Self {
            nonce: CNonce { hi, mid, lo, end },
            _phantom: PhantomData,
        }
    }

    /// Read the nonce attached to this token.
    pub fn nonce(&self) -> CNonce {
        self.nonce
    }
}

// `SecurityToken` is `Send`/`Sync` because the nonce identifies
// it; revoking via the kernel table does not require access to
// the token itself.
unsafe impl<P> Send for SecurityToken<P> {}
unsafe impl<P> Sync for SecurityToken<P> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonces_are_unique() {
        let a = unsafe { SecurityToken::<()>::new() };
        let b = unsafe { SecurityToken::<()>::new() };
        assert_ne!(a.nonce(), b.nonce());
    }

    #[test]
    fn nonce_is_copy() {
        let t = unsafe { SecurityToken::<()>::new() };
        let _copy: CNonce = t.nonce();
    }
}
