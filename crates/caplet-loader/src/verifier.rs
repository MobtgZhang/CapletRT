//! Module signature verification.
//!
//! A trusted key is selected by the manifest's `public_key_id`.  When the
//! `crypto` feature is enabled the signature is verified with
//! `ed25519-dalek` via [`caplet_crypto`].  Without that feature the verifier
//! returns [`SignatureError::Unavailable`]; callers must not treat that as
//! success.
//!
//! The signature covers a canonical, length-prefixed encoding of all security
//! relevant manifest fields followed by the module image.  Signing the image
//! alone would allow an attacker to change the capability manifest without
//! invalidating the signature.

extern crate alloc;

use alloc::vec::Vec;
use caplet_sync::SpinLock;

use crate::manifest::ModuleManifest;

const SIGNING_DOMAIN: &[u8] = b"CapletRT.Module.v1\0";

/// Errors emitted by the signature verifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureError {
    /// No key with the manifest's `public_key_id` is trusted.
    UnknownKey,
    /// The public key or signature is the wrong size.
    BadSize,
    /// Signature failed to verify.
    Invalid,
    /// No crypto backend is compiled in.
    Unavailable,
}

/// A trusted signing key.
#[derive(Debug, Clone)]
pub struct SigningKey {
    /// Id used to look up the key.
    pub id: u32,
    /// Ed25519 public key bytes (32 bytes).
    pub public_key: Vec<u8>,
}

impl SigningKey {
    /// Construct a key from a 32-byte Ed25519 public key.
    pub fn ed25519(id: u32, public_key: [u8; 32]) -> Self {
        Self {
            id,
            public_key: public_key.to_vec(),
        }
    }
}

/// The trust store. Hosts (boards, the loader CLI tool) populate this from
/// their root-of-trust configuration.
#[derive(Debug, Clone, Default)]
pub struct TrustStore {
    keys: Vec<SigningKey>,
}

impl TrustStore {
    /// Construct an empty trust store.
    pub const fn new() -> Self {
        Self { keys: Vec::new() }
    }

    /// Add a key.
    pub fn add(&mut self, key: SigningKey) {
        self.keys.push(key);
    }

    /// Look up a key by id.
    pub fn lookup(&self, id: u32) -> Option<&SigningKey> {
        self.keys.iter().find(|k| k.id == id)
    }

    /// Number of trusted keys.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the store contains no keys.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Global trust store. `None` until [`install_trust_store`] is called.
static TRUST_STORE: SpinLock<Option<TrustStore>> = SpinLock::new(None);

/// Install the trust store. Idempotent: subsequent calls are ignored so a
/// late, less-privileged caller cannot replace the boot root of trust.
pub fn install_trust_store(store: TrustStore) {
    let mut slot = TRUST_STORE.lock();
    if slot.is_none() {
        *slot = Some(store);
    }
}

/// Return a snapshot of the installed trust store.
///
/// A snapshot avoids handing out a reference protected by a spin lock. Trust
/// stores are small and are only read during module loading.
pub fn current_trust_store() -> Option<TrustStore> {
    TRUST_STORE.lock().clone()
}

/// Remove the global trust store. Intended for tests and secure teardown.
pub fn clear_trust_store() {
    *TRUST_STORE.lock() = None;
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    push_u32(out, bytes.len() as u32);
    out.extend_from_slice(bytes);
}

/// Build the canonical byte string covered by a module signature.
///
/// The encoding is deliberately independent of container section ordering.
/// Both the signer tool and the loader must use this function.
pub fn signing_message(manifest: &ModuleManifest, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(256 + payload.len());
    out.extend_from_slice(SIGNING_DOMAIN);
    out.push(match manifest.format {
        crate::manifest::ModuleFormat::Elf => 1,
        crate::manifest::ModuleFormat::Capp => 2,
    });
    out.extend_from_slice(&manifest.version.to_le_bytes());
    push_u32(&mut out, manifest.name_len as u32);
    out.extend_from_slice(&manifest.name);
    push_u32(&mut out, manifest.entry_point);
    push_u32(&mut out, manifest.stack_size);
    out.push(manifest.priority);
    out.push(manifest.signed as u8);
    out.push(manifest.relocatable as u8);
    push_u32(&mut out, manifest.text_base);
    push_u32(&mut out, manifest.text_size);
    push_u32(&mut out, manifest.data_size);
    push_u32(&mut out, manifest.public_key_id);

    for caps in [&manifest.required_caps, &manifest.granted_caps] {
        push_u32(&mut out, caps.len() as u32);
        for cap in caps.iter() {
            push_len_prefixed(&mut out, cap.tag.as_bytes());
            push_u32(&mut out, cap.perms);
            out.push(cap.safety);
        }
    }

    push_u32(&mut out, payload.len() as u32);
    out.extend_from_slice(payload);
    out
}

/// Verify the signature on `manifest` against `payload`.
///
/// `payload` is the module image (`.text + .data`). The manifest metadata is
/// included in the signed message by [`signing_message`].
pub fn verify_signature(manifest: &ModuleManifest, payload: &[u8]) -> Result<(), SignatureError> {
    let store = current_trust_store().ok_or(SignatureError::Unavailable)?;
    verify_signature_with_store(manifest, payload, &store)
}

/// Verify a signature against an explicit trust store.
///
/// This is the deterministic API used by tools and tests; it does not depend
/// on global state.
pub fn verify_signature_with_store(
    manifest: &ModuleManifest,
    payload: &[u8],
    store: &TrustStore,
) -> Result<(), SignatureError> {
    let key = store
        .lookup(manifest.public_key_id)
        .ok_or(SignatureError::UnknownKey)?;
    let message = signing_message(manifest, payload);
    crypto_verify(key, &manifest.signature, &message)
}

#[cfg(feature = "crypto")]
fn crypto_verify(key: &SigningKey, sig: &[u8], message: &[u8]) -> Result<(), SignatureError> {
    let public_key: [u8; 32] = key
        .public_key
        .as_slice()
        .try_into()
        .map_err(|_| SignatureError::BadSize)?;
    let signature: [u8; 64] = sig.try_into().map_err(|_| SignatureError::BadSize)?;

    if caplet_crypto::ed25519_verify(&public_key, message, &signature) {
        Ok(())
    } else {
        Err(SignatureError::Invalid)
    }
}

#[cfg(not(feature = "crypto"))]
fn crypto_verify(_key: &SigningKey, _sig: &[u8], _message: &[u8]) -> Result<(), SignatureError> {
    Err(SignatureError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{CapDescriptor, ModuleFormat};

    fn manifest() -> ModuleManifest {
        let mut manifest = ModuleManifest::new(ModuleFormat::Capp);
        manifest.name = [0u8; 32];
        manifest.name[..5].copy_from_slice(b"hello");
        manifest.name_len = 5;
        manifest.entry_point = 0x2000;
        manifest.text_base = 0x2000;
        manifest.text_size = 4;
        manifest.public_key_id = 7;
        manifest.signed = true;
        manifest
            .granted_caps
            .push(CapDescriptor::new("log", 0xff, 1));
        manifest.required_caps = manifest.granted_caps.clone();
        manifest
    }

    #[test]
    fn verify_rejects_when_no_trust_store_or_backend() {
        let mut manifest = manifest();
        manifest.signature = [1u8; 64];
        assert_eq!(
            verify_signature(&manifest, b"payload"),
            Err(SignatureError::Unavailable)
        );
    }

    #[test]
    fn unknown_key_is_rejected() {
        let manifest = manifest();
        let store = TrustStore::new();
        assert_eq!(
            verify_signature_with_store(&manifest, b"payload", &store),
            Err(SignatureError::UnknownKey)
        );
    }

    #[cfg(feature = "crypto")]
    #[test]
    fn accepts_real_signature_and_rejects_tampering() {
        use ed25519_dalek::{Signer, SigningKey as DalekSigningKey};

        let secret = DalekSigningKey::from_bytes(&[7u8; 32]);
        let public = secret.verifying_key().to_bytes();
        let mut manifest = manifest();
        let payload = b"module image";
        manifest.signature = secret.sign(&signing_message(&manifest, payload)).to_bytes();

        let mut store = TrustStore::new();
        store.add(SigningKey::ed25519(7, public));
        assert_eq!(
            verify_signature_with_store(&manifest, payload, &store),
            Ok(())
        );

        assert_eq!(
            verify_signature_with_store(&manifest, b"module imagE", &store),
            Err(SignatureError::Invalid)
        );
        manifest.entry_point += 1;
        assert_eq!(
            verify_signature_with_store(&manifest, payload, &store),
            Err(SignatureError::Invalid)
        );
    }

    #[cfg(feature = "crypto")]
    #[test]
    fn rejects_wrong_key_and_bad_key_length() {
        use ed25519_dalek::{Signer, SigningKey as DalekSigningKey};

        let signer = DalekSigningKey::from_bytes(&[9u8; 32]);
        let mut manifest = manifest();
        manifest.signature = signer.sign(&signing_message(&manifest, b"x")).to_bytes();

        let mut wrong_store = TrustStore::new();
        wrong_store.add(SigningKey::ed25519(
            7,
            DalekSigningKey::from_bytes(&[10u8; 32])
                .verifying_key()
                .to_bytes(),
        ));
        assert_eq!(
            verify_signature_with_store(&manifest, b"x", &wrong_store),
            Err(SignatureError::Invalid)
        );

        let mut short_store = TrustStore::new();
        short_store.add(SigningKey {
            id: 7,
            public_key: alloc::vec![0u8; 31],
        });
        assert_eq!(
            verify_signature_with_store(&manifest, b"x", &short_store),
            Err(SignatureError::BadSize)
        );
    }
}
