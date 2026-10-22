//! End-to-end loader tests: signed `.capp`, tamper rejection, capability
//! attenuation, relocation, and copying into a caller-owned mapping.

use caplet_cap::{CSpace, Permissions, SafetyBits};
use caplet_loader::allocator::ModulePool;
use caplet_loader::capp;
use caplet_loader::sandbox::{load_module_with_policy, LoaderError, SignaturePolicy};
use caplet_loader::verifier::{
    clear_trust_store, install_trust_store, signing_message, SigningKey, TrustStore,
};
use ed25519_dalek::{Signer, SigningKey as DalekSigningKey};

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

const KEY_ID: u16 = 7;

fn build_capp(
    image: &[u8],
    entry: u32,
    flags: u8,
    caps: &[(&str, u32)],
    relocs: &[(u32, u32)],
    signer: Option<&DalekSigningKey>,
) -> Vec<u8> {
    let text_off = capp::HEADER_SIZE;
    let data_off = text_off + image.len();
    let reloc_off = data_off;
    let cap_off = reloc_off + relocs.len() * 8;
    let name_off = cap_off + caps.len() * 32;
    let name = b"signed-module\0";

    let mut out = Vec::new();
    out.extend_from_slice(&capp::MAGIC);
    out.extend_from_slice(&capp::SUPPORTED_VERSION.to_le_bytes());
    out.extend_from_slice(&(capp::HEADER_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&entry.to_le_bytes());
    out.extend_from_slice(&4096u32.to_le_bytes());
    out.push(8);
    out.push(flags);
    out.extend_from_slice(&KEY_ID.to_le_bytes());
    out.extend_from_slice(&(text_off as u32).to_le_bytes());
    out.extend_from_slice(&(image.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(reloc_off as u32).to_le_bytes());
    out.extend_from_slice(&(relocs.len() as u32).to_le_bytes());
    out.extend_from_slice(&(cap_off as u32).to_le_bytes());
    out.extend_from_slice(&(caps.len() as u32).to_le_bytes());
    out.extend_from_slice(&(name_off as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; capp::SIGNATURE_SIZE]);
    assert_eq!(out.len(), capp::HEADER_SIZE);

    out.extend_from_slice(image);
    for &(vaddr, addend) in relocs {
        out.extend_from_slice(&vaddr.to_le_bytes());
        out.extend_from_slice(&addend.to_le_bytes());
    }
    for &(tag, perms) in caps {
        let mut descriptor = [0u8; 32];
        let bytes = tag.as_bytes();
        assert!(bytes.len() < 28);
        descriptor[..bytes.len()].copy_from_slice(bytes);
        descriptor[28..32].copy_from_slice(&perms.to_le_bytes());
        out.extend_from_slice(&descriptor);
    }
    out.extend_from_slice(name);
    while out.len() % 4 != 0 {
        out.push(0);
    }

    if let Some(signer) = signer {
        // The signature covers the canonical manifest plus the image, so
        // parse the unsigned container first and sign exactly what the
        // loader will reconstruct.
        let parsed = capp::parse_capp(&out).expect("parse unsigned container");
        let message = signing_message(&parsed.manifest, &parsed.image);
        let signature = signer.sign(&message).to_bytes();
        out[56..56 + capp::SIGNATURE_SIZE].copy_from_slice(&signature);
    }
    out
}

fn trust_store(signer: &DalekSigningKey) -> TrustStore {
    let mut store = TrustStore::new();
    store.add(SigningKey::ed25519(
        KEY_ID as u32,
        signer.verifying_key().to_bytes(),
    ));
    store
}

fn loader_caps(entries: &[(&str, u32)]) -> CSpace {
    let mut cspace = CSpace::new();
    for &(tag, perms) in entries {
        let handle = cspace
            .allocate(Permissions(perms), SafetyBits::default(), None)
            .unwrap();
        cspace.bind_name(handle.index(), tag).unwrap();
    }
    cspace
}

#[test]
fn signed_loader_end_to_end() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_trust_store();
    let signer = DalekSigningKey::from_bytes(&[7u8; 32]);
    let image = [0x11u8, 0x22, 0x33, 0x44];
    let flags = capp::FLAG_SIGNED | capp::FLAG_RELOCATABLE;
    let container = build_capp(&image, 0, flags, &[], &[], Some(&signer));

    // Enforce without a trust store must not silently accept the module.
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    assert_eq!(
        load_module_with_policy(
            &container,
            &mut pool,
            &loader_caps(&[]),
            SignaturePolicy::Enforce,
        )
        .err(),
        Some(LoaderError::CryptoUnavailable)
    );

    install_trust_store(trust_store(&signer));
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    let loaded = load_module_with_policy(
        &container,
        &mut pool,
        &loader_caps(&[]),
        SignaturePolicy::Enforce,
    )
    .expect("valid signature loads");
    assert_eq!(loaded.image(), &image);
    assert_eq!(loaded.entry_vaddr, loaded.base_vaddr);

    let mut mapping = [0u8; 8];
    loaded.copy_to(&mut mapping).unwrap();
    assert_eq!(&mapping[..4], &image);
    let mut too_small = [0u8; 2];
    assert_eq!(
        loaded.copy_to(&mut too_small).err(),
        Some(LoaderError::BadSize)
    );

    // Tampering with the image must invalidate the signature.
    let mut tampered_image = container.clone();
    tampered_image[capp::HEADER_SIZE] ^= 0xFF;
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    assert_eq!(
        load_module_with_policy(
            &tampered_image,
            &mut pool,
            &loader_caps(&[]),
            SignaturePolicy::Enforce,
        )
        .err(),
        Some(LoaderError::BadSignature)
    );

    // Tampering with the entry point changes the signed manifest.
    let mut tampered_entry = container.clone();
    tampered_entry[8..12].copy_from_slice(&1u32.to_le_bytes());
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    assert_eq!(
        load_module_with_policy(
            &tampered_entry,
            &mut pool,
            &loader_caps(&[]),
            SignaturePolicy::Enforce,
        )
        .err(),
        Some(LoaderError::BadSignature)
    );

    // Tampering with a capability descriptor is also detected.
    let caps = [("log", 0x3u32)];
    let with_caps = build_capp(&image, 0, flags, &caps, &[], Some(&signer));
    let mut tampered_caps = with_caps.clone();
    let cap_perms_off = capp::HEADER_SIZE + image.len() + 28;
    tampered_caps[cap_perms_off] ^= 0xFF;
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    assert_eq!(
        load_module_with_policy(
            &tampered_caps,
            &mut pool,
            &loader_caps(&[("log", 0x3)]),
            SignaturePolicy::Enforce,
        )
        .err(),
        Some(LoaderError::BadSignature)
    );

    // A module cannot require more authority than the loader holds.
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    assert_eq!(
        load_module_with_policy(
            &with_caps,
            &mut pool,
            &loader_caps(&[]),
            SignaturePolicy::Enforce,
        )
        .err(),
        Some(LoaderError::MissingCapability)
    );
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    assert_eq!(
        load_module_with_policy(
            &with_caps,
            &mut pool,
            &loader_caps(&[("log", 0x1)]),
            SignaturePolicy::Enforce,
        )
        .err(),
        Some(LoaderError::MissingCapability)
    );
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    let loaded = load_module_with_policy(
        &with_caps,
        &mut pool,
        &loader_caps(&[("log", 0x3)]),
        SignaturePolicy::Enforce,
    )
    .expect("sufficient loader capability");
    assert!(loaded.cspace.find_by_name("log").is_some());

    clear_trust_store();
}

#[test]
fn unsigned_policy_and_relocation() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_trust_store();
    let image = [0x11u8, 0x22, 0x33, 0x44];

    // Enforce rejects an unsigned module; AllowUnsigned is the explicit
    // development escape hatch.
    let unsigned = build_capp(&image, 0, capp::FLAG_RELOCATABLE, &[], &[], None);
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    assert_eq!(
        load_module_with_policy(
            &unsigned,
            &mut pool,
            &loader_caps(&[]),
            SignaturePolicy::Enforce,
        )
        .err(),
        Some(LoaderError::UnsignedModule)
    );
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    assert!(load_module_with_policy(
        &unsigned,
        &mut pool,
        &loader_caps(&[]),
        SignaturePolicy::AllowUnsigned,
    )
    .is_ok());

    // A relocatable entry point outside the image is rejected.
    let bad_entry = build_capp(
        &image,
        image.len() as u32,
        capp::FLAG_RELOCATABLE,
        &[],
        &[],
        None,
    );
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    assert_eq!(
        load_module_with_policy(
            &bad_entry,
            &mut pool,
            &loader_caps(&[]),
            SignaturePolicy::AllowUnsigned,
        )
        .err(),
        Some(LoaderError::BadEntry)
    );

    // R_ARM_RELATIVE-style relocation uses image offsets and adds the load
    // bias exactly once.
    let mut reloc_image = [0u8; 8];
    reloc_image[..4].copy_from_slice(&0x1000u32.to_le_bytes());
    let relocated = build_capp(
        &reloc_image,
        0,
        capp::FLAG_RELOCATABLE,
        &[],
        &[(0, 0)],
        None,
    );
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    let loaded = load_module_with_policy(
        &relocated,
        &mut pool,
        &loader_caps(&[]),
        SignaturePolicy::AllowUnsigned,
    )
    .expect("relocatable module loads");
    let mut mapping = [0u8; 8];
    loaded.copy_to(&mut mapping).unwrap();
    assert_eq!(
        u32::from_le_bytes(mapping[..4].try_into().unwrap()),
        0x1000 + 0x6000_0000
    );

    // A non-relocatable container with relocation entries is malformed.
    let non_reloc = build_capp(&reloc_image, 0, 0, &[], &[(0, 0)], None);
    let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
    assert_eq!(
        load_module_with_policy(
            &non_reloc,
            &mut pool,
            &loader_caps(&[]),
            SignaturePolicy::AllowUnsigned,
        )
        .err(),
        Some(LoaderError::BadReloc)
    );
}
