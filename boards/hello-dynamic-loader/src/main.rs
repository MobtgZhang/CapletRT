//! Hello-dynamic-loader demo for M3.
//!
//! This host-target binary builds `.capp` modules in memory, loads them with
//! the production loader, maps the relocated image executable, and calls its
//! entry point. It also checks unsigned-policy, bad-entry and signature-tamper
//! rejection so the demo exercises the security boundary rather than only the
//! parser.

use caplet_loader::allocator::ModulePool;
use caplet_loader::capp;
use caplet_loader::manifest::ModuleFormat;
use caplet_loader::parse_capp;
use caplet_loader::sandbox::{
    granted_caps_summary, load_module, load_module_with_policy, manifest_to_string, LoadedModule,
    LoaderError, SignaturePolicy,
};
use caplet_loader::verifier::{install_trust_store, signing_message, SigningKey, TrustStore};
use ed25519_dalek::{Signer, SigningKey as DalekSigningKey};

use caplet_cap::{CSpace, Permissions, SafetyBits};

const KEY_ID: u32 = 7;

#[cfg(target_arch = "x86_64")]
const RETURN_42: &[u8] = &[0xb8, 0x2a, 0x00, 0x00, 0x00, 0xc3];
#[cfg(target_arch = "aarch64")]
const RETURN_42: &[u8] = &[0x40, 0x05, 0x80, 0x52, 0xc0, 0x03, 0x5f, 0xd6];
#[cfg(target_arch = "arm")]
const RETURN_42: &[u8] = &[0x2a, 0x00, 0xa0, 0xe3, 0x1e, 0xff, 0x2f, 0xe1];
#[cfg(target_arch = "riscv64")]
const RETURN_42: &[u8] = &[0x13, 0x05, 0xa0, 0x02, 0x67, 0x80, 0x00, 0x00];

/// Build a relocatable `.capp` with one executable image and no relocations.
fn build_sample_capp(image: &[u8], entry: u32, flags: u8, signed: bool) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&capp::MAGIC);
    out.extend_from_slice(&capp::SUPPORTED_VERSION.to_le_bytes());
    out.extend_from_slice(&(capp::HEADER_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&entry.to_le_bytes());
    out.extend_from_slice(&4096u32.to_le_bytes()); // stack
    out.push(8); // priority
    out.push(flags);
    out.extend_from_slice(&(if signed { KEY_ID as u16 } else { 0 }).to_le_bytes());
    let text_off = capp::HEADER_SIZE;
    out.extend_from_slice(&(text_off as u32).to_le_bytes());
    out.extend_from_slice(&(image.len() as u32).to_le_bytes());
    let data_off = text_off + image.len();
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    let name_off = data_off;
    out.extend_from_slice(&(name_off as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; capp::SIGNATURE_SIZE]);
    out.extend_from_slice(image);
    out.extend_from_slice(b"hello-dynamic\0");
    out
}

fn sign_sample(out: &mut [u8], signer: &DalekSigningKey) {
    let parsed = parse_capp(out).expect("parse unsigned sample");
    let message = signing_message(&parsed.manifest, &parsed.image);
    let signature = signer.sign(&message).to_bytes();
    out[56..56 + capp::SIGNATURE_SIZE].copy_from_slice(&signature);
}

struct ExecutableMapping {
    ptr: *mut u8,
    len: usize,
}

impl ExecutableMapping {
    fn new(size: usize) -> Self {
        let page = 4096usize;
        let len = size.max(1).div_ceil(page) * page;
        let ptr = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            panic!("mmap failed: {}", std::io::Error::last_os_error());
        }
        Self {
            ptr: ptr.cast(),
            len,
        }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for ExecutableMapping {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.cast(), self.len);
        }
    }
}

fn execute_loaded(module: &LoadedModule) -> u32 {
    let mut mapping = ExecutableMapping::new(module.image().len());
    module.copy_to(mapping.as_mut_slice()).expect("copy module");
    let entry = unsafe { mapping.ptr.add(module.entry_offset() as usize) };
    let function: unsafe extern "C" fn() -> u32 = unsafe { core::mem::transmute(entry) };
    unsafe { function() }
}

fn load_unsigned(bytes: &[u8]) -> Result<LoadedModule, LoaderError> {
    let mut pool = ModulePool::new(0x6000_0000, 16 * 1024 * 1024);
    load_module_with_policy(
        bytes,
        &mut pool,
        &CSpace::new(),
        SignaturePolicy::AllowUnsigned,
    )
}

fn loader_caps_for(parsed: &capp::ParsedCapp) -> CSpace {
    let mut cspace = CSpace::new();
    for cap in &parsed.manifest.granted_caps {
        let handle = cspace
            .allocate(Permissions(cap.perms), SafetyBits::default(), None)
            .expect("allocate loader capability");
        cspace
            .bind_name(handle.index(), &cap.tag)
            .expect("bind loader capability name");
    }
    cspace
}

fn load_enforced(bytes: &[u8]) -> Result<LoadedModule, LoaderError> {
    let mut pool = ModulePool::new(0x6000_0000, 16 * 1024 * 1024);
    load_module(bytes, &mut pool, &CSpace::new())
}

fn run_external(path: &str) -> ! {
    let bytes = std::fs::read(path).unwrap_or_else(|error| {
        panic!("failed to read {path}: {error}");
    });
    let parsed = match parse_capp(&bytes) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("hello-dynamic-loader: external module parse failed: {error:?}");
            std::process::exit(1);
        }
    };
    println!("{}", manifest_to_string(&parsed.manifest));
    let cspace = loader_caps_for(&parsed);
    let mut pool = ModulePool::new(0x6000_0000, 16 * 1024 * 1024);
    let loaded =
        match load_module_with_policy(&bytes, &mut pool, &cspace, SignaturePolicy::AllowUnsigned) {
            Ok(loaded) => loaded,
            Err(error) => {
                eprintln!("hello-dynamic-loader: external module load failed: {error:?}");
                std::process::exit(1);
            }
        };
    let result = execute_loaded(&loaded);
    println!(
        "hello-dynamic-loader: external module {} executed -> {}",
        path, result
    );
    std::process::exit(if result == 42 { 0 } else { 1 });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--load" {
        run_external(&args[2]);
    }
    if args.len() != 1 {
        eprintln!("usage: hello-dynamic-loader [--load module.capp]");
        std::process::exit(2);
    }

    println!("hello-dynamic-loader: building relocatable .capp modules in memory");
    let unsigned = build_sample_capp(RETURN_42, 0, capp::FLAG_RELOCATABLE, false);

    let parsed = parse_capp(&unsigned).expect("parse_capp");
    if !matches!(parsed.manifest.format, ModuleFormat::Capp) {
        panic!("expected Capp, got {:?}", parsed.manifest.format);
    }
    println!("{}", manifest_to_string(&parsed.manifest));

    match load_enforced(&unsigned).err() {
        Some(LoaderError::UnsignedModule) => {
            println!("hello-dynamic-loader: unsigned module rejected by Enforce policy")
        }
        other => panic!("expected UnsignedModule, got {other:?}"),
    }

    let loaded = load_unsigned(&unsigned).expect("load unsigned sample");
    println!(
        "loaded unsigned module: entry=0x{:x} base=0x{:x} size={}",
        loaded.entry_vaddr, loaded.base_vaddr, loaded.image_size
    );
    let summary = granted_caps_summary(&loaded.manifest);
    println!("granted caps: {}", summary.len());
    for (tag, perms) in &summary {
        println!("  - {tag}: 0x{perms:x}");
    }
    assert_eq!(execute_loaded(&loaded), 42);
    println!("hello-dynamic-loader: executed entry -> 42");

    let bad_entry = build_sample_capp(RETURN_42, 0x1000, capp::FLAG_RELOCATABLE, false);
    match load_unsigned(&bad_entry).err() {
        Some(LoaderError::BadEntry) => {
            println!("hello-dynamic-loader: out-of-image entry rejected: BadEntry")
        }
        other => panic!("expected BadEntry, got {other:?}"),
    }

    let signer = DalekSigningKey::from_bytes(&[7u8; 32]);
    let mut store = TrustStore::new();
    store.add(SigningKey::ed25519(
        KEY_ID,
        signer.verifying_key().to_bytes(),
    ));
    install_trust_store(store);

    let mut signed = build_sample_capp(
        RETURN_42,
        0,
        capp::FLAG_RELOCATABLE | capp::FLAG_SIGNED,
        true,
    );
    sign_sample(&mut signed, &signer);
    let signed_loaded = load_enforced(&signed).expect("load signed sample");
    assert_eq!(execute_loaded(&signed_loaded), 42);
    println!("hello-dynamic-loader: signed module verified and executed -> 42");

    let mut tampered = signed;
    tampered[capp::HEADER_SIZE] ^= 0xff;
    match load_enforced(&tampered).err() {
        Some(LoaderError::BadSignature) => {
            println!("hello-dynamic-loader: tampered signed module rejected: BadSignature")
        }
        other => panic!("expected BadSignature, got {other:?}"),
    }

    println!("hello-dynamic-loader: PASS");
}
