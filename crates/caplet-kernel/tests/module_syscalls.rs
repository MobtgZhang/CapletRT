//! End-to-end tests for the dynamic module syscalls.
//!
//! The kernel and the loader's trust store are process-global, so every
//! scenario runs inside one serial test.

extern crate alloc;

use caplet_cap::permissions::perm;
use caplet_cap::SafetyBits;
use caplet_kernel::kernel::{init_kernel, with_kernel, CURRENT_TASK_ID};
use caplet_kernel::kernel_config::KernelConfig;
use caplet_kernel::process::{Process, ProcessId};
use caplet_kernel::syscall::{Syscall, WideSyscallArgs};
use caplet_kernel::syscall_dispatch::syscall_dispatch_wide;
use caplet_kernel::syscall_ipc::{ERR_BAD_ARG, ERR_DENIED, ERR_EXISTS, ERR_NOT_FOUND};
use caplet_kernel::syscall_m3::{
    ERR_MODULE_PARSE, ERR_MODULE_SIGNATURE, ERR_NOT_READY, ERR_NOT_REGISTERED,
};
use caplet_loader::capp;
use caplet_loader::parse_capp;
use caplet_loader::verifier::{install_trust_store, signing_message, SigningKey, TrustStore};
use core::sync::atomic::{AtomicU32, Ordering};
use ed25519_dalek::{Signer, SigningKey as DalekSigningKey};

const KEY_ID: u32 = 11;
const MODULE_POOL_BASE: u32 = 0x6000_0000;
const MODULE_POOL_SIZE: u32 = 1024 * 1024;

static MAPPED_BASE: AtomicU32 = AtomicU32::new(0);
static MAPPED_LEN: AtomicU32 = AtomicU32::new(0);
static EXECUTED_ENTRY: AtomicU32 = AtomicU32::new(0);

fn map_hook(base: u32, image: &[u8]) -> bool {
    MAPPED_BASE.store(base, Ordering::Release);
    MAPPED_LEN.store(image.len() as u32, Ordering::Release);
    true
}

unsafe extern "C" fn execute_hook(entry: u32, arg: u32) -> u32 {
    EXECUTED_ENTRY.store(entry, Ordering::Release);
    42 + arg
}

fn create_process(name: &str) -> (ProcessId, u32) {
    let pid = with_kernel(|kernel| {
        kernel.create_process(Process::new(ProcessId(0), String::from(name), 0, 0, 0))
    })
    .expect("kernel");
    let task = 200 + pid.0;
    with_kernel(|kernel| {
        kernel.set_task_id(pid, Some(task)).expect("task binding");
    })
    .expect("kernel");
    (pid, task)
}

fn install_cap(pid: ProcessId, name: &str) -> u32 {
    with_kernel(|kernel| {
        let process = kernel.get_process_mut(pid).expect("process");
        let handle = process
            .cspace
            .allocate(perm::READ, SafetyBits::default(), None)
            .expect("cap slot");
        process
            .cspace
            .bind_name(handle.index(), name)
            .expect("cap name");
        handle.index()
    })
    .expect("kernel")
}

fn build_capp(image: &[u8], flags: u8) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&capp::MAGIC);
    out.extend_from_slice(&capp::SUPPORTED_VERSION.to_le_bytes());
    out.extend_from_slice(&(capp::HEADER_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // entry, relative to image
    out.extend_from_slice(&4096u32.to_le_bytes());
    out.push(8);
    out.push(flags);
    out.extend_from_slice(&(KEY_ID as u16).to_le_bytes());
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
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; capp::SIGNATURE_SIZE]);
    out.extend_from_slice(image);
    out.extend_from_slice(b"module-test\0");
    out
}

fn sign_capp(bytes: &mut [u8], signer: &DalekSigningKey) {
    let parsed = parse_capp(bytes).expect("parse unsigned capp");
    let message = signing_message(&parsed.manifest, &parsed.image);
    let signature = signer.sign(&message).to_bytes();
    bytes[56..56 + capp::SIGNATURE_SIZE].copy_from_slice(&signature);
}

fn module_load(bytes: &[u8]) -> caplet_kernel::syscall::SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::ModuleLoad.number(),
        bytes.len() as u32,
        bytes.as_ptr() as u64,
        MODULE_POOL_BASE as u64,
        MODULE_POOL_SIZE,
    ))
}

fn module_register(id: u32) -> caplet_kernel::syscall::SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::ModuleRegister.number(),
        id,
        0,
        0,
        0,
    ))
}

fn module_start(id: u32, arg: u32) -> caplet_kernel::syscall::SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::ModuleStart.number(),
        id,
        arg as u64,
        0,
        0,
    ))
}

#[test]
fn module_syscalls_enforce_capabilities_signatures_and_lifecycle() {
    unsafe { init_kernel(KernelConfig::default()) };
    let (pid, task) = create_process("module-host");
    CURRENT_TASK_ID.store(task as usize, Ordering::Release);

    let signer = DalekSigningKey::from_bytes(&[11u8; 32]);
    let mut trust = TrustStore::new();
    trust.add(SigningKey::ed25519(
        KEY_ID,
        signer.verifying_key().to_bytes(),
    ));
    install_trust_store(trust);

    let image = [0x90u8, 0x90, 0x90, 0xc3];
    let mut signed = build_capp(&image, capp::FLAG_RELOCATABLE | capp::FLAG_SIGNED);
    sign_capp(&mut signed, &signer);
    let unsigned = build_capp(&image, capp::FLAG_RELOCATABLE);

    // Loading requires an explicit kernel-issued capability.
    assert_eq!(module_load(&signed).error, ERR_DENIED);
    install_cap(pid, "module:load");

    // No board/host hooks means the kernel must fail closed rather than
    // pretending the image was mapped.
    assert_eq!(module_load(&signed).error, ERR_NOT_READY);
    with_kernel(|kernel| {
        kernel
            .modules
            .set_hooks(caplet_kernel::module::ModuleHooks {
                map: map_hook,
                execute: execute_hook,
            });
    })
    .expect("kernel");

    // The syscall path always uses SignaturePolicy::Enforce.
    assert_eq!(module_load(&unsigned).error, ERR_MODULE_SIGNATURE);
    assert_eq!(module_load(b"not a module").error, ERR_MODULE_PARSE);

    let loaded = module_load(&signed);
    assert!(loaded.success, "load signed module: {loaded:?}");
    let id = loaded.value;
    assert_eq!(MAPPED_LEN.load(Ordering::Acquire), image.len() as u32);
    assert_eq!(MAPPED_BASE.load(Ordering::Acquire), MODULE_POOL_BASE);

    // Execution is a separate, capability-checked lifecycle step.
    assert_eq!(module_start(id, 0).error, ERR_DENIED);
    install_cap(pid, "module:start");
    assert_eq!(module_start(id, 0).error, ERR_NOT_REGISTERED);

    assert_eq!(module_register(id).error, ERR_DENIED);
    install_cap(pid, "module:register");
    assert!(module_register(id).success);
    assert_eq!(module_register(id).error, ERR_EXISTS);
    assert_eq!(module_register(9999).error, ERR_NOT_FOUND);

    let started = module_start(id, 7);
    assert!(started.success, "start module: {started:?}");
    assert_eq!(started.value, 49);
    assert_eq!(EXECUTED_ENTRY.load(Ordering::Acquire), MODULE_POOL_BASE);

    // Bad pointers and unknown ids stay well-defined errors.
    assert_eq!(
        syscall_dispatch_wide(WideSyscallArgs::new(
            Syscall::ModuleLoad.number(),
            0,
            0xdead_beef,
            MODULE_POOL_BASE as u64,
            MODULE_POOL_SIZE,
        ))
        .error,
        ERR_BAD_ARG
    );
    assert_eq!(module_start(9999, 0).error, ERR_NOT_FOUND);

    // A conflicting pool range is rejected instead of silently moving the
    // persistent allocator.
    let conflict = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::ModuleLoad.number(),
        signed.len() as u32,
        signed.as_ptr() as u64,
        (MODULE_POOL_BASE + 0x1000) as u64,
        MODULE_POOL_SIZE,
    ));
    assert_eq!(conflict.error, caplet_kernel::syscall_m3::ERR_MODULE_POOL);
}
