//! End-to-end namespace IPC authorization tests.
//!
//! The kernel and `CURRENT_TASK_ID` are process-global, so this file keeps
//! all scenarios in one serial test rather than racing independent tests.

extern crate alloc;

use caplet_budget::Budget;
use caplet_cap::permissions::perm;
use caplet_cap::{Permissions, SafetyBits};
use caplet_kernel::ipc_namespace::NamespaceId;
use caplet_kernel::kernel::{init_kernel, with_kernel, CURRENT_TASK_ID};
use caplet_kernel::kernel_config::KernelConfig;
use caplet_kernel::process::{Process, ProcessId};
use caplet_kernel::syscall::{Syscall, SyscallResult, WideSyscallArgs};
use caplet_kernel::syscall_dispatch::syscall_dispatch_wide;
use caplet_kernel::syscall_ipc::{
    grant_endpoint_access, ERR_DENIED, ERR_EXISTS, ERR_NOT_FOUND, ERR_QUOTA,
};
use core::sync::atomic::Ordering;

fn create_process(name: &str) -> (ProcessId, u32) {
    let pid = with_kernel(|kernel| {
        kernel.create_process(Process::new(ProcessId(0), String::from(name), 0, 0, 0))
    })
    .expect("kernel");
    let task = 100 + pid.0;
    with_kernel(|kernel| {
        kernel.set_task_id(pid, Some(task)).expect("task binding");
    })
    .expect("kernel");
    (pid, task)
}

fn switch_to(task: u32) {
    CURRENT_TASK_ID.store(task as usize, Ordering::Release);
}

fn install_cap(pid: ProcessId, name: &str, permissions: Permissions, safety: SafetyBits) -> u32 {
    with_kernel(|kernel| {
        let process = kernel.get_process_mut(pid).expect("process");
        let handle = process
            .cspace
            .allocate(permissions, safety, None)
            .expect("cap slot");
        process
            .cspace
            .bind_name(handle.index(), name)
            .expect("cap name");
        handle.index()
    })
    .expect("kernel")
}

fn create_namespace(name: &[u8], parent: u32) -> SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::NsCreate.number(),
        name.len() as u32,
        name.as_ptr() as u64,
        parent as u64,
        0,
    ))
}

fn join_namespace(namespace: u32) -> SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::NsJoin.number(),
        namespace,
        0,
        0,
        0,
    ))
}

fn register_endpoint(name: &[u8], namespace: u32) -> SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::NsEndpoint.number(),
        name.len() as u32,
        name.as_ptr() as u64,
        namespace as u64,
        0,
    ))
}

fn subscribe(name: &[u8], namespace: u32) -> SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::NsSubscribe.number(),
        name.len() as u32,
        name.as_ptr() as u64,
        namespace as u64,
        0,
    ))
}

fn publish(name: &[u8], payload: &[u8]) -> SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::NsPublish.number(),
        name.len() as u32,
        name.as_ptr() as u64,
        payload.as_ptr() as u64,
        payload.len() as u32,
    ))
}

fn receive(name: &[u8], buffer: &mut [u8]) -> SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::IpcRecv.number(),
        name.len() as u32,
        name.as_ptr() as u64,
        buffer.as_mut_ptr() as u64,
        buffer.len() as u32,
    ))
}

fn endpoint(name: &str) -> caplet_kernel::ipc_namespace::EndpointName {
    caplet_kernel::ipc_namespace::EndpointName(String::from(name))
}

#[test]
fn namespace_ipc_security_and_abi() {
    unsafe { init_kernel(KernelConfig::default()) };

    let (publisher, publisher_task) = create_process("publisher");
    let (subscriber, subscriber_task) = create_process("subscriber");
    let (outsider, outsider_task) = create_process("outsider");
    switch_to(publisher_task);

    // Creating a namespace requires an explicit kernel-issued capability.
    let ns_name = b"factory";
    assert_eq!(create_namespace(ns_name, u32::MAX).error, ERR_DENIED);
    install_cap(publisher, "ns:create", perm::READ, SafetyBits::default());
    let created = create_namespace(ns_name, u32::MAX);
    assert!(created.success, "NsCreate: {created:?}");
    assert!(created.value > 0);
    let ns = created.value;

    // The creator receives an `ns:<id>` capability, so it may join its own
    // namespace and register endpoints there.
    assert!(join_namespace(ns).success);
    let endpoint_name = b"sensors/temp";
    assert!(register_endpoint(endpoint_name, ns).success);
    assert_eq!(
        register_endpoint(endpoint_name, ns).error,
        ERR_EXISTS,
        "duplicate registration must not redirect an existing endpoint"
    );

    // Registration no longer self-authorizes the creator.
    assert_eq!(publish(endpoint_name, b"22.5").error, ERR_DENIED);

    grant_endpoint_access(
        publisher,
        NamespaceId(ns),
        &endpoint("sensors/temp"),
        true,
        false,
        false,
    )
    .expect("publisher grant");
    let sent = publish(endpoint_name, b"22.5");
    assert!(sent.success, "authorized publish: {sent:?}");

    // Joining requires the namespace capability and the target must exist.
    switch_to(subscriber_task);
    assert_eq!(
        join_namespace(u32::MAX).error,
        ERR_NOT_FOUND,
        "NsJoin must reject an unallocated namespace id"
    );
    assert_eq!(join_namespace(ns).error, ERR_DENIED);
    install_cap(
        subscriber,
        &format!("ns:{ns}"),
        perm::READ,
        SafetyBits::default(),
    );
    assert!(join_namespace(ns).success);

    // Receiving is denied until the endpoint grants read access explicitly.
    let mut buffer = [0u8; 32];
    assert_eq!(receive(endpoint_name, &mut buffer).error, ERR_DENIED);
    grant_endpoint_access(
        subscriber,
        NamespaceId(ns),
        &endpoint("sensors/temp"),
        false,
        true,
        false,
    )
    .expect("subscriber grant");
    assert!(subscribe(endpoint_name, ns).success);
    let received = receive(endpoint_name, &mut buffer);
    assert!(received.success, "receive: {received:?}");
    assert_eq!(received.value >> 16, 4);
    assert_eq!(&buffer[..4], b"22.5");

    // A process in the namespace without a subscriber capability cannot
    // drain another endpoint's queue.
    switch_to(outsider_task);
    install_cap(
        outsider,
        &format!("ns:{ns}"),
        perm::READ,
        SafetyBits::default(),
    );
    assert!(join_namespace(ns).success);
    assert_eq!(receive(endpoint_name, &mut buffer).error, ERR_DENIED);

    // A child namespace may reach a root endpoint, but publishing across the
    // boundary additionally requires the cross_namespace safety bit.
    switch_to(publisher_task);
    install_cap(publisher, "ns:0", perm::READ, SafetyBits::default());
    let root_endpoint = b"root/events";
    let root_registered = register_endpoint(root_endpoint, 0);
    assert!(
        root_registered.success,
        "root endpoint: {root_registered:?}"
    );

    let (cross_publisher, cross_task) = create_process("cross-publisher");
    switch_to(cross_task);
    install_cap(
        cross_publisher,
        &format!("ns:{ns}"),
        perm::READ,
        SafetyBits::default(),
    );
    assert!(join_namespace(ns).success);
    grant_endpoint_access(
        cross_publisher,
        NamespaceId(0),
        &endpoint("root/events"),
        true,
        false,
        false,
    )
    .expect("same-name restricted grant");
    assert_eq!(publish(root_endpoint, b"tick").error, ERR_DENIED);
    grant_endpoint_access(
        cross_publisher,
        NamespaceId(0),
        &endpoint("root/events"),
        true,
        false,
        true,
    )
    .expect("cross-namespace grant");
    assert!(publish(root_endpoint, b"tick").success);

    // Quota exhaustion is reported before a message enters a queue.
    let (limited, limited_task) = create_process("limited");
    switch_to(limited_task);
    install_cap(
        limited,
        &format!("ns:{ns}"),
        perm::READ,
        SafetyBits::default(),
    );
    assert!(join_namespace(ns).success);
    grant_endpoint_access(
        limited,
        NamespaceId(ns),
        &endpoint("sensors/temp"),
        true,
        false,
        false,
    )
    .expect("limited publisher grant");
    with_kernel(|kernel| {
        kernel
            .get_process_mut(limited)
            .expect("process")
            .set_budget(Budget {
                memory_bytes: u32::MAX,
                cpu_us_per_quantum: caplet_budget::MicroSeconds::MAX,
                messages_per_sec: 0,
            });
    })
    .expect("kernel");
    assert_eq!(publish(endpoint_name, b"quota").error, ERR_QUOTA);

    CURRENT_TASK_ID.store(0, Ordering::Release);
}
