//! IPC syscall handlers.
//!
//! There are two IPC families:
//! - anonymous in-kernel channels (`ChannelCreate/Send/Recv`), and
//! - namespace-scoped named endpoints (`IpcSend/Recv/Reply` and the
//!   `Ns*` operations).
//!
//! Named endpoints are keyed by `(NamespaceId, EndpointName)`. A send
//! resolves the name through the namespace tree (so parent-namespace
//! endpoints remain reachable), enforces the namespace publisher
//! allowlist, queues the message, and wakes a receiver that previously
//! blocked with `IpcRecv`. Replies use a per-task context and inbox;
//! the previous single global reply slot could cross-deliver between
//! unrelated tasks.

extern crate alloc;

use crate::ipc::{DeliveryOutcome, IpcMessage};
use crate::ipc_namespace::{EndpointHandle, EndpointName, IpcNamespace, NamespaceId};
use crate::kernel::{current_pid, with_current_cspace, with_kernel, CURRENT_TASK_ID};
use crate::process::ProcessId;
use crate::quota;
use crate::syscall::{Syscall, SyscallArgs, SyscallResult, WideSyscallArgs};
use ::caplet_cap::permissions::{perm, Permissions};
use ::caplet_cap::{CSpace, SafetyBits};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// IPC/quota errno space.  These values are part of the userspace ABI.
pub const ERR_NOT_INIT: u32 = 20;
/// The requested endpoint or namespace does not exist.
pub const ERR_NOT_FOUND: u32 = 21;
/// A pointer, length, or namespace id is invalid.
pub const ERR_BAD_ARG: u32 = 22;
/// The operation would block.
pub const ERR_WOULD_BLOCK: u32 = 23;
/// A receive found no queued message.
pub const ERR_QUEUE_EMPTY: u32 = 24;
/// The caller-provided buffer is too small.
pub const ERR_PAYLOAD_TOO_BIG: u32 = 25;
/// There is no pending request context for this task.
pub const ERR_NO_REPLY_CONTEXT: u32 = 26;
/// A required capability, allowlist entry, or safety bit is missing.
pub const ERR_DENIED: u32 = 27;
/// The process message quota is exhausted.
pub const ERR_QUOTA: u32 = 28;
/// An endpoint already exists with this namespace/name pair.
pub const ERR_EXISTS: u32 = 29;
/// The namespace or caller CSpace has no room for a new capability.
pub const ERR_NO_SPACE: u32 = 30;

const MAX_ENDPOINT_NAME: usize = 256;
const MAX_PAYLOAD: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct EndpointKey {
    namespace: NamespaceId,
    name: EndpointName,
}

struct IpcEndpoint {
    messages: VecDeque<IpcMessage>,
    /// Task ids that returned `ERR_WOULD_BLOCK` from `IpcRecv` and are
    /// waiting for a message on this endpoint.
    waiting_receivers: VecDeque<u32>,
}

impl IpcEndpoint {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            waiting_receivers: VecDeque::new(),
        }
    }
}

static IPC_ENDPOINTS: SpinLock<BTreeMap<EndpointKey, IpcEndpoint>> = SpinLock::new(BTreeMap::new());
static NEXT_ENDPOINT_ID: AtomicU32 = AtomicU32::new(1);

struct ReplyContext {
    sender_id: u32,
    endpoint: EndpointName,
    seq: u64,
}

/// Per-receiver reply context and per-target reply inbox.
static REPLY_CONTEXTS: SpinLock<BTreeMap<u32, ReplyContext>> = SpinLock::new(BTreeMap::new());
static REPLY_INBOXES: SpinLock<BTreeMap<u32, VecDeque<Vec<u8>>>> = SpinLock::new(BTreeMap::new());

struct AsyncEntry {
    sender: caplet_sync::Sender<Vec<u8>>,
    receiver: caplet_sync::Receiver<Vec<u8>>,
}

struct SyncEntry {
    sender: caplet_sync::SyncSender<Vec<u8>>,
    receiver: caplet_sync::SyncReceiver<Vec<u8>>,
}

enum ChannelEntry {
    Async(AsyncEntry),
    Bounded(SyncEntry),
}

static CHANNELS: SpinLock<BTreeMap<u32, ChannelEntry>> = SpinLock::new(BTreeMap::new());
static NEXT_CHANNEL_ID: AtomicU32 = AtomicU32::new(1);

fn current_task() -> u32 {
    CURRENT_TASK_ID.load(Ordering::Acquire) as u32
}

/// `(pid, namespace)` for the calling task. Kernel/boot tasks are
/// mapped to `ProcessId(0)` and the root namespace.
fn current_identity() -> (ProcessId, NamespaceId) {
    let pid = current_pid().unwrap_or(ProcessId(0));
    let ns = with_kernel(|kernel| {
        kernel
            .get_process(pid)
            .map(|process| process.ipc_namespace)
            .unwrap_or(NamespaceId(0))
    })
    .unwrap_or(NamespaceId(0));
    (pid, ns)
}

fn read_name(ptr: usize, len: usize) -> Result<EndpointName, u32> {
    if ptr == 0 || len == 0 || len > MAX_ENDPOINT_NAME {
        return Err(ERR_BAD_ARG);
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    let text = core::str::from_utf8(bytes).map_err(|_| ERR_BAD_ARG)?;
    Ok(EndpointName(String::from(text)))
}

fn read_payload(ptr: usize, len: usize) -> Result<Vec<u8>, u32> {
    if ptr == 0 || len == 0 {
        return Err(ERR_BAD_ARG);
    }
    if len > MAX_PAYLOAD {
        return Err(ERR_PAYLOAD_TOO_BIG);
    }
    Ok(unsafe { core::slice::from_raw_parts(ptr as *const u8, len) }.to_vec())
}

fn resolve(namespace: NamespaceId, name: &EndpointName) -> Option<(NamespaceId, u32)> {
    with_kernel(|kernel| {
        kernel
            .namespaces
            .resolve(namespace, name)
            .map(|(ns, handle)| (ns, handle.0))
    })
    .flatten()
}

fn namespace_cap_name(namespace: NamespaceId) -> String {
    format!("ns:{}", namespace.0)
}

fn endpoint_cap_name(namespace: NamespaceId, name: &EndpointName) -> String {
    format!("endpoint:{}:{}", namespace.0, name.0)
}

fn cspace_has_named_cap(
    cspace: &CSpace,
    name: &str,
    required: Permissions,
    require_cross_namespace: bool,
) -> bool {
    cspace.find_all_by_name(name).into_iter().any(|index| {
        cspace
            .slot_perms_safety(index)
            .map(|(permissions, safety)| {
                permissions.contains(required)
                    && (!require_cross_namespace || safety.cross_namespace())
            })
            .unwrap_or(false)
    })
}

fn process_cspace(pid: ProcessId) -> Option<CSpace> {
    with_kernel(|kernel| {
        kernel
            .get_process(pid)
            .map(|process| process.cspace.clone())
            .or_else(|| (pid == ProcessId(0)).then(|| kernel.cspace.clone()))
    })
    .flatten()
}

fn process_has_named_cap(
    pid: ProcessId,
    name: &str,
    required: Permissions,
    require_cross_namespace: bool,
) -> bool {
    process_cspace(pid)
        .map(|cspace| cspace_has_named_cap(&cspace, name, required, require_cross_namespace))
        .unwrap_or(false)
}

fn current_has_named_cap(name: &str, required: Permissions, require_cross_namespace: bool) -> bool {
    with_current_cspace(|cspace| {
        cspace_has_named_cap(cspace, name, required, require_cross_namespace)
    })
}

fn namespace_cap_allowed(namespace: NamespaceId) -> bool {
    current_has_named_cap(&namespace_cap_name(namespace), perm::READ, false)
}

fn endpoint_subscribe_allowed(namespace: NamespaceId, name: &EndpointName) -> bool {
    current_has_named_cap(&endpoint_cap_name(namespace, name), perm::READ, false)
}

/// Kernel/loader helper that explicitly grants endpoint access to a process.
///
/// The grant installs a live endpoint capability in the target process's
/// CSpace and records the publisher/subscriber allowlist entries.  Endpoint
/// registration deliberately does not call this function: creating an
/// endpoint is not evidence that its owner may send to or receive from it.
pub fn grant_endpoint_access(
    pid: ProcessId,
    namespace: NamespaceId,
    name: &EndpointName,
    can_publish: bool,
    can_subscribe: bool,
    cross_namespace: bool,
) -> Result<u32, u32> {
    if !can_publish && !can_subscribe {
        return Err(ERR_BAD_ARG);
    }
    let exists = with_kernel(|kernel| {
        kernel
            .namespaces
            .get(namespace)
            .map(|ns| ns.endpoints.contains_key(name))
            .unwrap_or(false)
    })
    .unwrap_or(false);
    if !exists {
        return Err(ERR_NOT_FOUND);
    }

    let mut permissions = Permissions::empty();
    if can_publish {
        permissions |= perm::WRITE;
    }
    if can_subscribe {
        permissions |= perm::READ;
    }
    permissions |= perm::DERIVE | perm::TRANSFER;
    let mut safety = SafetyBits::default();
    safety.set_cross_namespace(cross_namespace);
    let cap_name = endpoint_cap_name(namespace, name);

    let slot = with_kernel(|kernel| {
        let cspace = match kernel.processes.get_mut(pid) {
            Some(process) => &mut process.cspace,
            None if pid == ProcessId(0) => &mut kernel.cspace,
            None => return Err(ERR_NOT_FOUND),
        };
        let handle = cspace
            .allocate(permissions, safety, None)
            .map_err(|_| ERR_NO_SPACE)?;
        cspace
            .bind_name(handle.index(), &cap_name)
            .map_err(|_| ERR_NO_SPACE)?;
        Ok(handle.index())
    })
    .unwrap_or(Err(ERR_NOT_INIT))?;

    with_kernel(|kernel| {
        let Some(ns) = kernel.namespaces.get_mut(namespace) else {
            return;
        };
        if can_publish && !ns.allowed_publishers.contains(&pid) {
            ns.allowed_publishers.push(pid);
        }
        if can_subscribe && !ns.allowed_subscribers.contains(&pid) {
            ns.allowed_subscribers.push(pid);
        }
    });
    Ok(slot)
}

fn make_task_ready(task: u32) {
    if task == 0 {
        return;
    }
    let _ = with_kernel(|kernel| kernel.scheduler().make_ready(task));
}

fn suspend_task(task: u32) {
    if let Some(pid) = with_kernel(|kernel| {
        kernel
            .processes
            .iter()
            .find_map(|(pid, process)| (process.task_id == Some(task)).then_some(*pid))
    })
    .flatten()
    {
        let _ = with_kernel(|kernel| {
            if let Some(process) = kernel.get_process_mut(pid) {
                process.suspend();
            }
        });
    }
}

/// Register a named endpoint in `namespace`.
fn register_endpoint(namespace: NamespaceId, name: EndpointName) -> Result<u32, u32> {
    let mut namespace_exists = false;
    let mut duplicate = false;
    with_kernel(|kernel| {
        let Some(ns) = kernel.namespaces.get_mut(namespace) else {
            return;
        };
        namespace_exists = true;
        duplicate = ns.endpoints.contains_key(&name);
    });
    if !namespace_exists {
        return Err(ERR_NOT_FOUND);
    }
    if duplicate {
        return Err(ERR_EXISTS);
    }
    let handle = NEXT_ENDPOINT_ID.fetch_add(1, Ordering::AcqRel);
    if handle == 0 {
        return Err(ERR_NOT_INIT);
    }
    let inserted = with_kernel(|kernel| {
        let Some(ns) = kernel.namespaces.get_mut(namespace) else {
            return false;
        };
        ns.register(name.clone(), EndpointHandle(handle))
    })
    .unwrap_or(false);
    if !inserted {
        return Err(ERR_EXISTS);
    }
    IPC_ENDPOINTS
        .lock()
        .entry(EndpointKey { namespace, name })
        .or_insert_with(IpcEndpoint::new);
    Ok(handle)
}

fn deliver(
    namespace: NamespaceId,
    name: EndpointName,
    pid: ProcessId,
    from_ns: NamespaceId,
    payload: Vec<u8>,
) -> DeliveryOutcome {
    let resolved = match resolve(namespace, &name) {
        Some(r) => r,
        None => return DeliveryOutcome::UnknownEndpoint,
    };
    let target_ns = resolved.0;
    let listed = with_kernel(|kernel| {
        kernel
            .namespaces
            .get(target_ns)
            .map(|ns| ns.may_publish(pid))
            .unwrap_or(false)
    })
    .unwrap_or(false);
    let has_cross_cap = process_has_named_cap(
        pid,
        &endpoint_cap_name(target_ns, &name),
        perm::WRITE,
        target_ns != from_ns,
    );
    let permission = listed && has_cross_cap;
    if !permission {
        return DeliveryOutcome::Denied;
    }

    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) as u64;
    let key = EndpointKey {
        namespace: target_ns,
        name: name.clone(),
    };
    let mut endpoints = IPC_ENDPOINTS.lock();
    let Some(endpoint) = endpoints.get_mut(&key) else {
        return DeliveryOutcome::UnknownEndpoint;
    };
    endpoint
        .messages
        .push_back(IpcMessage::new(pid, from_ns, name, seq, payload));
    let waiter = endpoint.waiting_receivers.pop_front();
    drop(endpoints);
    if let Some(task) = waiter {
        make_task_ready(task);
    }
    DeliveryOutcome::Delivered
}

fn read_wide_ptr(lo: u32, hi: u32) -> usize {
    (((hi as u64) << 32) | lo as u64) as usize
}

fn namespace_from_arg(value: u32) -> NamespaceId {
    if value == u32::MAX {
        current_identity().1
    } else {
        NamespaceId(value)
    }
}

/// Handle IPC and namespace syscalls with the narrow (32-bit) ABI.
pub fn handle_ipc_syscall(args: SyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::ChannelCreate => channel_create(args.arg0),
        Syscall::ChannelSend => channel_send(args.arg0, args.arg1 as usize, args.arg2 as usize),
        Syscall::ChannelRecv => channel_recv(args.arg0, args.arg2 as usize, args.arg3 as usize),
        Syscall::IpcSend => ipc_send(
            args.arg0 as usize,
            args.arg1 as usize,
            args.arg2 as usize,
            args.arg3 as usize,
        ),
        Syscall::IpcRecv => ipc_recv(
            args.arg0 as usize,
            args.arg1 as usize,
            args.arg2 as usize,
            args.arg3 as usize,
        ),
        Syscall::IpcReply => ipc_reply(args.arg0 as usize, args.arg1 as usize),
        Syscall::NsEndpoint => ns_endpoint(
            args.arg0 as usize,
            args.arg1 as usize,
            namespace_from_arg(args.arg2),
        ),
        Syscall::NsSubscribe => ns_subscribe(
            args.arg0 as usize,
            args.arg1 as usize,
            namespace_from_arg(args.arg2),
        ),
        Syscall::NsPublish => ns_publish(
            args.arg0 as usize,
            args.arg1 as usize,
            args.arg2 as usize,
            args.arg3 as usize,
        ),
        _ => SyscallResult::err(1),
    }
}

fn channel_create(capacity: u32) -> SyscallResult {
    let id = NEXT_CHANNEL_ID.fetch_add(1, Ordering::AcqRel);
    if id == 0 {
        return SyscallResult::err(ERR_NOT_INIT);
    }
    let entry = if capacity == 0 {
        let (sender, receiver) = caplet_sync::Channel::<Vec<u8>>::unbounded();
        ChannelEntry::Async(AsyncEntry { sender, receiver })
    } else {
        let (sender, receiver) = caplet_sync::SyncChannel::<Vec<u8>>::bounded(capacity as usize);
        ChannelEntry::Bounded(SyncEntry { sender, receiver })
    };
    CHANNELS.lock().insert(id, entry);
    SyscallResult::ok(id)
}

fn channel_send(channel: u32, ptr: usize, len: usize) -> SyscallResult {
    if ptr == 0 || len == 0 || len > MAX_PAYLOAD {
        return SyscallResult::err(ERR_BAD_ARG);
    }
    let bytes = match read_payload(ptr, len) {
        Ok(bytes) => bytes,
        Err(error) => return SyscallResult::err(error),
    };
    let channels = CHANNELS.lock();
    let Some(entry) = channels.get(&channel) else {
        return SyscallResult::err(ERR_NOT_FOUND);
    };
    if quota::charge_current_message().is_err() {
        return SyscallResult::err(ERR_QUOTA);
    }
    let result = match entry {
        ChannelEntry::Async(e) => e.sender.try_send(bytes).map_err(|_| ()),
        ChannelEntry::Bounded(e) => e.sender.try_send(bytes).map_err(|_| ()),
    };
    match result {
        Ok(()) => SyscallResult::ok(len as u32),
        Err(()) => {
            quota::refund_current_message();
            SyscallResult::err(ERR_WOULD_BLOCK)
        }
    }
}

fn channel_recv(channel: u32, ptr: usize, capacity: usize) -> SyscallResult {
    if ptr == 0 || capacity == 0 {
        return SyscallResult::err(ERR_BAD_ARG);
    }
    let channels = CHANNELS.lock();
    let Some(entry) = channels.get(&channel) else {
        return SyscallResult::err(ERR_NOT_FOUND);
    };
    let message = match entry {
        ChannelEntry::Async(e) => e.receiver.try_recv().ok(),
        ChannelEntry::Bounded(e) => e.receiver.try_recv().ok(),
    };
    match message {
        Some(message) if message.len() <= capacity => {
            unsafe {
                core::ptr::copy_nonoverlapping(message.as_ptr(), ptr as *mut u8, message.len())
            };
            SyscallResult::ok(message.len() as u32)
        }
        Some(_) => SyscallResult::err(ERR_PAYLOAD_TOO_BIG),
        None => SyscallResult::err(ERR_WOULD_BLOCK),
    }
}

fn ipc_send(
    name_ptr: usize,
    name_len: usize,
    payload_ptr: usize,
    payload_len: usize,
) -> SyscallResult {
    let name = match read_name(name_ptr, name_len) {
        Ok(name) => name,
        Err(error) => return SyscallResult::err(error),
    };
    let payload = match read_payload(payload_ptr, payload_len) {
        Ok(payload) => payload,
        Err(error) => return SyscallResult::err(error),
    };
    if quota::charge_current_message().is_err() {
        return SyscallResult::err(ERR_QUOTA);
    }
    let (pid, namespace) = current_identity();
    match deliver(namespace, name, pid, namespace, payload) {
        DeliveryOutcome::Delivered => SyscallResult::ok(0),
        DeliveryOutcome::NoSubscribers => {
            quota::refund_current_message();
            SyscallResult::ok(1)
        }
        DeliveryOutcome::Denied => {
            quota::refund_current_message();
            SyscallResult::err(ERR_DENIED)
        }
        DeliveryOutcome::UnknownEndpoint => {
            quota::refund_current_message();
            SyscallResult::err(ERR_NOT_FOUND)
        }
    }
}

fn ipc_recv(name_ptr: usize, name_len: usize, buffer_ptr: usize, capacity: usize) -> SyscallResult {
    if buffer_ptr == 0 || capacity == 0 {
        return SyscallResult::err(ERR_BAD_ARG);
    }
    let receiver = current_task();

    // A reply to a previous request takes precedence over new endpoint
    // traffic so a request/reply round trip is deterministic.
    let reply = REPLY_INBOXES
        .lock()
        .get_mut(&receiver)
        .and_then(|queue| queue.pop_front());
    if let Some(payload) = reply {
        if payload.len() > capacity {
            REPLY_INBOXES
                .lock()
                .entry(receiver)
                .or_default()
                .push_front(payload);
            return SyscallResult::err(ERR_PAYLOAD_TOO_BIG);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(payload.as_ptr(), buffer_ptr as *mut u8, payload.len())
        };
        return SyscallResult::ok(payload.len() as u32);
    }

    let name = match read_name(name_ptr, name_len) {
        Ok(name) => name,
        Err(error) => return SyscallResult::err(error),
    };
    let (pid, namespace) = current_identity();
    let Some((target_ns, _)) = resolve(namespace, &name) else {
        return SyscallResult::err(ERR_NOT_FOUND);
    };
    let may_receive = with_kernel(|kernel| {
        kernel
            .namespaces
            .get(target_ns)
            .map(|ns| ns.may_subscribe(pid))
            .unwrap_or(false)
    })
    .unwrap_or(false)
        && endpoint_subscribe_allowed(target_ns, &name);
    if !may_receive {
        return SyscallResult::err(ERR_DENIED);
    }
    let key = EndpointKey {
        namespace: target_ns,
        name: name.clone(),
    };
    let mut endpoints = IPC_ENDPOINTS.lock();
    let Some(endpoint) = endpoints.get_mut(&key) else {
        return SyscallResult::err(ERR_NOT_FOUND);
    };
    match endpoint.messages.pop_front() {
        Some(message) if message.payload.len() > capacity => {
            endpoint.messages.push_front(message);
            SyscallResult::err(ERR_PAYLOAD_TOO_BIG)
        }
        Some(message) => {
            let len = message.payload.len();
            unsafe {
                core::ptr::copy_nonoverlapping(message.payload.as_ptr(), buffer_ptr as *mut u8, len)
            };
            REPLY_CONTEXTS.lock().insert(
                receiver,
                ReplyContext {
                    sender_id: message.from.0,
                    endpoint: name,
                    seq: message.seq,
                },
            );
            SyscallResult::ok((len as u32) << 16 | (message.from.0 & 0xFFFF))
        }
        None => {
            if !endpoint.waiting_receivers.contains(&receiver) {
                endpoint.waiting_receivers.push_back(receiver);
            }
            drop(endpoints);
            suspend_task(receiver);
            let _ = pid;
            SyscallResult::err(ERR_QUEUE_EMPTY)
        }
    }
}

fn ipc_reply(buffer_ptr: usize, buffer_len: usize) -> SyscallResult {
    let payload = match read_payload(buffer_ptr, buffer_len) {
        Ok(payload) => payload,
        Err(error) => return SyscallResult::err(error),
    };
    let receiver = current_task();
    let context = REPLY_CONTEXTS.lock().remove(&receiver);
    let Some(context) = context else {
        return SyscallResult::err(ERR_NO_REPLY_CONTEXT);
    };
    let target = context.sender_id;
    let len = payload.len();
    REPLY_INBOXES
        .lock()
        .entry(target)
        .or_default()
        .push_back(payload);
    make_task_ready(target);
    let _ = (context.endpoint, context.seq);
    SyscallResult::ok(len as u32)
}

fn ns_endpoint(name_ptr: usize, name_len: usize, namespace: NamespaceId) -> SyscallResult {
    let name = match read_name(name_ptr, name_len) {
        Ok(name) => name,
        Err(error) => return SyscallResult::err(error),
    };
    let (pid, current_ns) = current_identity();
    // A process may only create endpoints in its own namespace or an
    // ancestor it can already reach.
    if !namespace_is_self_or_ancestor(current_ns, namespace) {
        return SyscallResult::err(ERR_DENIED);
    }
    if !namespace_cap_allowed(namespace) {
        return SyscallResult::err(ERR_DENIED);
    }
    let _ = pid;
    match register_endpoint(namespace, name) {
        Ok(handle) => SyscallResult::ok(handle),
        Err(error) => SyscallResult::err(error),
    }
}

fn ns_subscribe(name_ptr: usize, name_len: usize, namespace: NamespaceId) -> SyscallResult {
    let name = match read_name(name_ptr, name_len) {
        Ok(name) => name,
        Err(error) => return SyscallResult::err(error),
    };
    let (pid, current_ns) = current_identity();
    let Some((target_ns, _)) = resolve(namespace, &name) else {
        return SyscallResult::err(ERR_NOT_FOUND);
    };
    if !namespace_is_self_or_ancestor(current_ns, target_ns) {
        return SyscallResult::err(ERR_DENIED);
    }
    if !endpoint_subscribe_allowed(target_ns, &name) {
        return SyscallResult::err(ERR_DENIED);
    }
    let ok = with_kernel(|kernel| {
        let Some(ns) = kernel.namespaces.get_mut(target_ns) else {
            return false;
        };
        if !ns.allowed_subscribers.contains(&pid) {
            ns.allowed_subscribers.push(pid);
        }
        true
    })
    .unwrap_or(false);
    if ok {
        SyscallResult::ok(0)
    } else {
        SyscallResult::err(ERR_NOT_FOUND)
    }
}

fn ns_publish(
    name_ptr: usize,
    name_len: usize,
    payload_ptr: usize,
    payload_len: usize,
) -> SyscallResult {
    let name = match read_name(name_ptr, name_len) {
        Ok(name) => name,
        Err(error) => return SyscallResult::err(error),
    };
    let payload = match read_payload(payload_ptr, payload_len) {
        Ok(payload) => payload,
        Err(error) => return SyscallResult::err(error),
    };
    if quota::charge_current_message().is_err() {
        return SyscallResult::err(ERR_QUOTA);
    }
    let (pid, namespace) = current_identity();
    match deliver(namespace, name, pid, namespace, payload) {
        DeliveryOutcome::Delivered => SyscallResult::ok(0),
        DeliveryOutcome::NoSubscribers => {
            quota::refund_current_message();
            SyscallResult::ok(1)
        }
        DeliveryOutcome::Denied => {
            quota::refund_current_message();
            SyscallResult::err(ERR_DENIED)
        }
        DeliveryOutcome::UnknownEndpoint => {
            quota::refund_current_message();
            SyscallResult::err(ERR_NOT_FOUND)
        }
    }
}

fn namespace_is_self_or_ancestor(current: NamespaceId, candidate: NamespaceId) -> bool {
    if current == candidate {
        return true;
    }
    with_kernel(|kernel| {
        let mut cursor = Some(current);
        while let Some(id) = cursor {
            let Some(ns): Option<&IpcNamespace> = kernel.namespaces.get(id) else {
                break;
            };
            if ns.parent == Some(candidate) {
                return true;
            }
            cursor = ns.parent;
        }
        false
    })
    .unwrap_or(false)
}

/// Wide-argument entry point for 64-bit host tests.
///
/// Wide ABI for named IPC (the narrow ABI cannot carry two pointers):
/// - `IpcSend`:  arg0 = name length, arg1 = name pointer,
///   arg2 = payload pointer, arg3 = payload length
/// - `IpcRecv`:  arg0 = name length, arg1 = name pointer,
///   arg2 = buffer pointer, arg3 = capacity
/// - `IpcReply`: arg0 = 0, arg1 = payload pointer,
///   arg2 = 0, arg3 = payload length
pub fn handle_ipc_syscall_wide(args: WideSyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::ChannelCreate => channel_create(args.arg0),
        Syscall::ChannelSend => channel_send(
            args.arg0,
            read_wide_ptr(args.arg1_lo, args.arg1_hi),
            args.arg2_lo as usize,
        ),
        Syscall::ChannelRecv => channel_recv(
            args.arg0,
            read_wide_ptr(args.arg2_lo, args.arg2_hi),
            args.arg3 as usize,
        ),
        Syscall::IpcSend => ipc_send(
            read_wide_ptr(args.arg1_lo, args.arg1_hi),
            args.arg0 as usize,
            read_wide_ptr(args.arg2_lo, args.arg2_hi),
            args.arg3 as usize,
        ),
        Syscall::IpcRecv => ipc_recv(
            read_wide_ptr(args.arg1_lo, args.arg1_hi),
            args.arg0 as usize,
            read_wide_ptr(args.arg2_lo, args.arg2_hi),
            args.arg3 as usize,
        ),
        Syscall::IpcReply => ipc_reply(
            read_wide_ptr(args.arg1_lo, args.arg1_hi),
            args.arg3 as usize,
        ),
        Syscall::NsEndpoint => ns_endpoint(
            read_wide_ptr(args.arg1_lo, args.arg1_hi),
            args.arg0 as usize,
            namespace_from_arg(args.arg2_lo),
        ),
        Syscall::NsSubscribe => ns_subscribe(
            read_wide_ptr(args.arg1_lo, args.arg1_hi),
            args.arg0 as usize,
            namespace_from_arg(args.arg2_lo),
        ),
        Syscall::NsPublish => ns_publish(
            read_wide_ptr(args.arg1_lo, args.arg1_hi),
            args.arg0 as usize,
            read_wide_ptr(args.arg2_lo, args.arg2_hi),
            args.arg3 as usize,
        ),
        _ => SyscallResult::err(1),
    }
}

// Small helper so the `?` operator above can be used without importing
// `From<u32>` for `SyscallResult`.
impl From<u32> for SyscallResult {
    fn from(error: u32) -> Self {
        SyscallResult::err(error)
    }
}
