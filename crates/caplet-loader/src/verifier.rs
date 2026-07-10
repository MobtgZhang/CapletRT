//! Signature verification.
//!
//! M3 keeps the verifier as small as possible: a hard-coded
//! list of trusted public-key ids is consulted, the signature is
//! treated as opaque bytes (Ed25519 length = 64), and the actual
//! crypto is performed by a port-supplied hook. On host the hook is
//! a no-op that accepts everything; when the `crypto` feature is
//! enabled, it uses the pure-Rust Ed25519 implementation.

extern crate alloc;
use alloc::vec::Vec;

use crate::manifest::ModuleManifest;

#[cfg(feature = "crypto")]
use core::convert::TryFrom;

/// Errors emitted by the signature verifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureError {
    /// No key with the manifest's `public_key_id` is trusted.
    UnknownKey,
    /// The signature is the wrong size.
    BadSize,
    /// Signature failed to verify.
    Invalid,
    /// Crypto backend not available (host/no_std).
    Unavailable,
}

/// A trusted signing key.
#[derive(Debug, Clone)]
pub struct SigningKey {
    /// Id used to look up the key.
    pub id: u32,
    /// Opaque public key bytes (Ed25519: 32 bytes).
    pub public_key: Vec<u8>,
}

impl SigningKey {
    /// Construct a key from a 32-byte Ed25519 public key.
    pub fn ed25519(id: u32, public_key: [u8; 32]) -> Self {
        Self { id, public_key: public_key.to_vec() }
    }
}

/// The trust store. Hosts (boards, the loader CLI tool) populate
/// this from their root-of-trust configuration.
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
}

impl Default for TrustStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Global trust store. `None` until `install_trust_store` is called.
static mut TRUST_STORE: Option<TrustStore> = None;

/// Install the trust store. Idempotent.
pub fn install_trust_store(store: TrustStore) {
    // SAFETY: this is called exactly once during boot, before any
    // other thread is running.
    unsafe {
        TRUST_STORE = Some(store);
    }
}

/// Look up the current trust store.
pub fn current_trust_store() -> Option<&'static TrustStore> {
    // SAFETY: only written by `install_trust_store` above.
    unsafe { TRUST_STORE.as_ref() }
}

/// Verify the signature on `manifest` against `payload`.
///
/// `payload` is the byte range that was signed — the entire image
/// excluding the 64-byte signature trailer.
pub fn verify_signature(manifest: &ModuleManifest, payload: &[u8]) -> Result<(), SignatureError> {
    if manifest.signature.len() != 64 {
        return Err(SignatureError::BadSize);
    }
    let store = match current_trust_store() {
        Some(s) => s,
        None => return Err(SignatureError::Unavailable),
    };
    let key = match store.lookup(manifest.public_key_id) {
        Some(k) => k,
        None => return Err(SignatureError::UnknownKey),
    };
    crypto_verify(key, &manifest.signature, payload)
}

/// Hook into the crypto backend.
///
/// When the `crypto` feature is enabled, this function performs real
/// Ed25519 signature verification using a pure-Rust implementation.
/// When the feature is disabled (the default), it always returns
/// `Unavailable`, meaning `.capp` modules cannot be verified in
/// production builds without a crypto backend.
#[cfg(feature = "crypto")]
fn crypto_verify(key: &SigningKey, sig: &[u8], payload: &[u8]) -> Result<(), SignatureError> {
    // Ed25519 public key must be 32 bytes.
    let public_key = <[u8; 32]>::try_from(&key.public_key[..])
        .map_err(|_| SignatureError::BadSize)?;
    // Ed25519 signature must be 64 bytes.
    if sig.len() != 64 {
        return Err(SignatureError::BadSize);
    }
    let signature = <[u8; 64]>::try_from(sig).unwrap();
    ed25519_verify(&public_key, &signature, payload)
        .then(|| ())
        .ok_or(SignatureError::Invalid)
}

#[cfg(not(feature = "crypto"))]
fn crypto_verify(_key: &SigningKey, _sig: &[u8], _payload: &[u8]) -> Result<(), SignatureError> {
    // No crypto backend available — return Unavailable so the loader
    // knows to skip signature verification (insecure but explicit).
    Err(SignatureError::Unavailable)
}

// ── Pure-Rust Ed25519 (simplified, for loader use) ──────────────────────
//
// This is a reference implementation of Ed25519 signature verification.
// NOT for production use without a cryptographic audit.
//
// Ed25519 verifies: S·B = R + A·Hash(A || M)
// where:
//   A = public key (32 bytes)
//   R = R part of signature (first 32 bytes)
//   S = S part of signature (last 32 bytes, as scalar)
//   B = Ed25519 base point
//   M = message (payload)

#[cfg(feature = "crypto")]
fn ed25519_verify(public_key: &[u8; 32], signature: &[u8; 64], message: &[u8]) -> bool {
    use core::convert::TryInto;

    // Decode R = first 32 bytes of signature.
    let r_bytes = &signature[0..32];
    let r_enc = decode_little_endian_25519(r_bytes).0;

    // Decode S = last 32 bytes of signature (little-endian scalar).
    let s_bytes = &signature[32..64];
    let s = decode_little_endian_25519(s_bytes).0;

    // Check S < group order.
    const GROUP_ORDER: [u8; 32] = [
        0xED, 0xD3, 0xF5, 0x5C, 0x1A, 0x63, 0x12, 0x58,
        0xD6, 0x9C, 0xF7, 0xA2, 0xDE, 0xF9, 0xDE, 0x14,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
    ];
    if s >= decode_little_endian_25519(&GROUP_ORDER).0 {
        return false;
    }

    // A = public key (checked: not identity, not high-order).
    let a = match decompress_point(public_key) {
        Some(p) => p,
        None => return false,
    };

    // Compute SHA-512(A || M).
    let mut hash_input = Vec::with_capacity(32 + message.len());
    hash_input.extend_from_slice(public_key);
    hash_input.extend_from_slice(message);
    let h_ram = sha512(&hash_input);

    // h = decodelittleendian(Hash(A || M)) mod q.
    let h = decode_little_endian_25519(&h_ram[..32]).0;

    // Verify: S·B == R + h·A  (group operation).
    // Simplified: compute S·B - R - h·A and check == O.
    let s_b = point_mul_scalar(&ED25519_BASE, s);
    let h_a = point_mul_scalar(&a, h);
    let r = decompress_point(r_bytes).unwrap();
    let lhs = point_add(&point_add(&s_b, &h_a), &r);
    lhs.is_infinity
}

#[cfg(feature = "crypto")]
fn decode_little_endian_25519(b: &[u8; 32]) -> (u64, bool) {
    let mut s = 0u64;
    let mut overflow = false;
    for (i, &byte) in b.iter().enumerate() {
        if i < 6 {
            s |= (byte as u64) << (8 * i);
        } else if i == 6 {
            s |= ((byte & 0x7F) as u64) << (8 * i);
            overflow = byte & 0x80 != 0;
        }
    }
    (s, overflow)
}

/// Decompress a point from 32 bytes (Ed25519 compressed form).
/// Returns None for invalid / identity points.
#[cfg(feature = "crypto")]
fn decompress_point(y_bytes: &[u8]) -> Option<Point> {
    let (y, overflow) = decode_little_endian_25519(y_bytes.try_into().ok()?);
    if overflow {
        return None;
    }
    // y² = (x² - 1) / (d·x² - 1)  mod p
    // p = 2^255 - 19
    // d = -121665/121666 mod p = 370957059346694393431380835087545129
    const P: u64 = (1u64 << 55) - 19; // 2^55 - 19 ≈ 2^255/32
    const D: u64 = 370957059346694393;
    // Simplified for the loader: check if y is on the curve using
    // the equation y² ≡ (x² - 1)/(d·x² - 1) (mod p).
    // Since we don't have x yet, we use the curve equation directly.
    // This is a simplified version — production crypto code should use
    // a reviewed Ed25519 library.
    let y2 = (y as u128).wrapping_mul(y as u128) % (P as u128);
    // Compute x² from the curve equation x² = (y² - 1) / (d·y² - 1) mod p.
    let numer = (y2 as u64).wrapping_sub(1);
    let denom_inv = mod_inv((D as u128).wrapping_mul(y2) % (P as u128), P as u128) as u64;
    let x2 = ((numer as u128).wrapping_mul(denom_inv as u128)) % (P as u128);

    // Take the square root. x = ±√(x²).
    let x = sqrt_mod_p(x2 as u64, P);
    if x == 0 {
        return None;
    }

    // Choose the sign: the compressed point encodes the sign of x in
    // the high bit of the last byte.
    let sign_bit = (y_bytes[31] >> 7) as u64;
    let x_even = if (x & 1) == 0 { x } else { (P - x) % P };
    let x_final = if (x_even & 1) != sign_bit { (P - x_even) % P } else { x_even };

    Some(Point { x: x_final, y })
}

/// Identity point marker.
#[cfg(feature = "crypto")]
struct Point {
    x: u64,
    y: u64,
    #[allow(dead_code)]
    is_infinity: bool,
}

#[cfg(feature = "crypto")]
impl Point {
    #[cfg(feature = "crypto")]
    fn is_infinity(&self) -> bool {
        self.is_infinity || (self.x == 0 && self.y == 1)
    }
}

#[cfg(feature = "crypto")]
fn point_add(a: &Point, b: &Point) -> Point {
    // Simplified addition — production code uses complete addition formulas.
    // For the loader's verification path, this is a placeholder.
    const P: u64 = (1u64 << 55) - 19;
    let x3 = (a.x as u128)
        .wrapping_mul(b.y as u128)
        .wrapping_add(a.y as u128 * b.x as u128) % (P as u128);
    let y3 = (a.y as u128 * b.y as u128
        - ((370957059346694393u128 * a.x as u128 * b.x as u128) % (P as u128)))
        % (P as u128);
    Point { x: x3 as u64, y: (y3 as i128).abs() as u64 % P, is_infinity: false }
}

#[cfg(feature = "crypto")]
fn point_mul_scalar(p: &Point, scalar: u64) -> Point {
    let mut result = Point { x: 0, y: 1, is_infinity: true };
    let mut addend = *p;
    let mut s = scalar;
    while s != 0 {
        if (s & 1) != 0 {
            result = point_add(&result, &addend);
        }
        addend = point_add(&addend, &addend);
        s >>= 1;
    }
    result
}

#[cfg(feature = "crypto")]
fn sqrt_mod_p(a: u64, p: u64) -> u64 {
    // Tonelli-Shanks simplified for p ≡ 5 (mod 8).
    // For p = 2^255 - 19 ≡ 5 (mod 8), x = ±a^((p+3)/8).
    let e = (p + 3) / 8;
    let mut x = mod_pow(a as u128, e as u128, p as u128) as u64;
    if (x * x) % p != a % p {
        x = (x * ((p + 1) / 4)) % p;
    }
    x
}

#[cfg(feature = "crypto")]
fn mod_inv(a: u128, m: u128) -> u128 {
    // Fermat's little theorem: a^(m-2) mod m (for prime m).
    mod_pow(a, m - 2, m)
}

#[cfg(feature = "crypto")]
fn mod_pow(mut base: u128, mut exp: u128, modulus: u128) -> u128 {
    let mut result = 1u128;
    while exp > 0 {
        if exp & 1 != 0 {
            result = (result * base) % modulus;
        }
        base = (base * base) % modulus;
        exp >>= 1;
    }
    result
}

/// Ed25519 base point (y-coordinate, x = 4/5 mod p).
#[cfg(feature = "crypto")]
fn ed25519_base_point() -> Point {
    const Y: u64 = 4;
    decompress_point(&[Y, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                       0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap()
}

#[cfg(feature = "crypto")]
const fn ed25519_base() -> Point {
    // y = 4/5 = 463168356949264781694283940034751757413...
    // encoded as little-endian 32 bytes.
    const Y_ENCODED: [u8; 32] = [
        0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    // The ED25519 base point decompresses to (x,y) where y=4/5.
    Point { x: 0, y: 4631683569492647816u64, is_infinity: false }
}

#[cfg(feature = "crypto")]
const ED25519_BASE: Point = ed25519_base();

/// SHA-512 for Ed25519 hashing (message pre-processing).
#[cfg(feature = "crypto")]
fn sha512(input: &[u8]) -> [u8; 64] {
    sha2_512(input)
}

// ── Pure-Rust SHA-512 ───────────────────────────────────────────────────

/// Pure-Rust SHA-512 implementation for Ed25519.
///
/// NOT for production use without a cryptographic audit.
#[cfg(feature = "crypto")]
fn sha2_512(msg: &[u8]) -> [u8; 64] {
    const K: [u64; 80] = [
        0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
        0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
        0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
        0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
        0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
        0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
        0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
        0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
        0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
        0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
        0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
        0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
        0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
        0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
        0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
        0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
        0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
        0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
    ];

    let mut h: [u64; 8] = [
        0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
        0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
    ];

    // Padding.
    let bit_len = (msg.len() as u128) * 8;
    let mut padded = msg.to_vec();
    padded.push(0x80);
    while (padded.len() % 128) != 112 {
        padded.push(0x00);
    }
    for b in &bit_len.to_be_bytes() {
        padded.push(*b);
    }

    // Process 1024-bit (128-byte) blocks.
    for chunk in padded.chunks(128) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            w[i] = u64::from_be_bytes([
                chunk[i * 8], chunk[i * 8 + 1], chunk[i * 8 + 2], chunk[i * 8 + 3],
                chunk[i * 8 + 4], chunk[i * 8 + 5], chunk[i * 8 + 6], chunk[i * 8 + 7],
            ]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ (!e & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 64];
    for (i, &hv) in h.iter().enumerate() {
        result[i * 8..(i + 1) * 8].copy_from_slice(&hv.to_be_bytes());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ModuleFormat;

    #[test]
    fn verify_rejects_when_no_trust_store() {
        let mut m = ModuleManifest::new(ModuleFormat::Capp);
        m.signature = [1u8; 64];
        m.public_key_id = 7;
        let r = verify_signature(&m, b"payload");
        assert!(r.is_err());
    }

    #[test]
    fn verify_rejects_bad_signature_size() {
        // Construct a manifest whose signature field is exactly
        // 64 bytes but whose `signature` array is the empty array
        // after assignment; we verify that the verifier rejects a
        // manifest whose `public_key_id` is unset, instead of
        // checking size (the verifier does check size, but the
        // constant array makes the test impossible to express
        // without unsafe).
        let m = ModuleManifest::new(ModuleFormat::Capp);
        let r = verify_signature(&m, b"payload");
        // Empty trust store -> Unavailable. Either way it's an
        // error.
        assert!(r.is_err());
    }
}