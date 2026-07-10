//! IPC syscall handlers
//!
//! In M1/M2 we use a global channel table indexed by the channel id.
//! Each entry holds a `Sender` and `Receiver` (sender is cloned for
//! every send, receiver is kept unique to the kernel).
//!
//! For task-to-task IPC, we maintain per-endpoint queues where tasks
//! can block waiting for messages.

use crate::syscall::{Syscall, SyscallArgs, SyscallResult, WideSyscallArgs};
use crate::ipc::IpcMessage;
use crate::ipc_namespace::{EndpointName, NamespaceId};
use crate::process::ProcessId;
use crate::kernel::{with_kernel, CURRENT_TASK_ID};
use ::caplet_sync::{Channel, SpinLock, SyncChannel};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

const ERR_NOT_INIT: u32 = 20;
const ERR_NOT_FOUND: u32 = 21;
const ERR_BAD_ARG: u32 = 22;
const ERR_WOULD_BLOCK: u32 = 23;
const ERR_QUEUE_EMPTY: u32 = 24;
const ERR_PAYLOAD_TOO_BIG: u32 = 25;
const ERR_NO_REPLY_CONTEXT: u32 = 26;

/// Maximum IPC payload size
const MAX_PAYLOAD: usize = 4096;

/// Per-endpoint IPC queue entry
struct IpcEndpoint {
    /// Message queue for this endpoint
    messages: VecDeque<IpcMessage>,
    /// Number of waiting receivers (for blocking receive)
    waiting_receivers: usize,
}

impl IpcEndpoint {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            waiting_receivers: 0,
        }
    }
}

/// Global IPC endpoint table (name -> endpoint)
struct IpcTable {
    inner: SpinLock<BTreeMap<EndpointName, IpcEndpoint>>,
}

impl IpcTable {
    const fn new() -> Self {
        Self { inner: SpinLock::new(BTreeMap::new()) }
    }

    fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut BTreeMap<EndpointName, IpcEndpoint>) -> R,
    {
        let mut g = self.inner.lock();
        f(&mut *g)
    }
}

static IPC_ENDPOINTS: IpcTable = IpcTable::new();
static NEXT_ENDPOINT_ID: AtomicU32 = AtomicU32::new(1);

/// Per-task reply context (set when task is waiting for reply after send)
struct ReplyContext {
    /// The original sender's task ID
    sender_id: u32,
    /// The endpoint name sent to
    endpoint: EndpointName,
    /// Message sequence number
    seq: u64,
}

static CURRENT_REPLY: SpinLock<Option<ReplyContext>> = SpinLock::new(None);

/// Async channel entry.
struct AsyncEntry {
    sender: ::caplet_sync::Sender<Vec<u8>>,
    receiver: ::caplet_sync::Receiver<Vec<u8>>,
}

/// Bounded sync channel entry.
struct SyncEntry {
    sender: ::caplet_sync::SyncSender<Vec<u8>>,
    receiver: ::caplet_sync::SyncReceiver<Vec<u8>>,
}

enum ChannelEntry {
    Async(AsyncEntry),
    Bounded(SyncEntry),
}

struct ChannelTable {
    inner: SpinLock<BTreeMap<u32, ChannelEntry>>,
}

impl ChannelTable {
    const fn new() -> Self {
        Self { inner: SpinLock::new(BTreeMap::new()) }
    }
    fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut BTreeMap<u32, ChannelEntry>) -> R,
    {
        let mut g = self.inner.lock();
        f(&mut *g)
    }
}

static CHANNELS: ChannelTable = ChannelTable::new();
static NEXT_CHANNEL_ID: AtomicU32 = AtomicU32::new(1);

/// Handle IPC-related syscalls.
pub fn handle_ipc_syscall(args: SyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::ChannelCreate => {
            let cap = args.arg0;
            let id = NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed);
            let entry = if cap == 0 {
                let (s, r) = Channel::<Vec<u8>>::unbounded();
                ChannelEntry::Async(AsyncEntry { sender: s, receiver: r })
            } else {
                let (s, r) = SyncChannel::<Vec<u8>>::new(cap as usize);
                ChannelEntry::Bounded(SyncEntry { sender: s, receiver: r })
            };
            CHANNELS.with(|t| {
                t.insert(id, entry);
            });
            SyscallResult::ok(id)
        }
        Syscall::ChannelSend => {
            let cid = args.arg0;
            let ptr = args.arg1 as *const u8;
            let len = args.arg2 as usize;
            if ptr.is_null() || len == 0 {
                return SyscallResult::err(ERR_BAD_ARG);
            }
            // We need a `&[u8]` with a real lifetime; the slice must be
            // valid for the duration of the call. On a 32-bit target the
            // pointer's high bits are zero so the cast is lossless; on
            // 64-bit hosts the caller is responsible for placing the
            // slice at a low address (e.g. via a static).
            let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
            // Copy into a heap Vec so the channel owns it.
            let bytes = bytes.to_vec();
            let mut out = SyscallResult::err(ERR_WOULD_BLOCK);
            let mut found = false;
            CHANNELS.with(|t| {
                if let Some(entry) = t.get(&cid) {
                    found = true;
                    let r = match entry {
                        ChannelEntry::Async(e) => e.sender.try_send(bytes).map_err(|_e| ()),
                        ChannelEntry::Bounded(e) => e.sender.try_send(bytes).map_err(|_e| ()),
                    };
                    out = match r {
                        Ok(()) => SyscallResult::ok(len as u32),
                        Err(_) => SyscallResult::err(ERR_WOULD_BLOCK),
                    };
                }
            });
            if found { out } else { SyscallResult::err(ERR_NOT_FOUND) }
        }
        Syscall::ChannelRecv => {
            let cid = args.arg0;
            let ptr = args.arg1 as *mut u8;
            let cap = args.arg2 as usize;
            if ptr.is_null() || cap == 0 {
                return SyscallResult::err(ERR_BAD_ARG);
            }
            let mut out = SyscallResult::err(ERR_WOULD_BLOCK);
            let mut found = false;
            CHANNELS.with(|t| {
                if let Some(entry) = t.get(&cid) {
                    found = true;
                    let msg = match entry {
                        ChannelEntry::Async(e) => e.receiver.try_recv().ok(),
                        ChannelEntry::Bounded(e) => e.receiver.try_recv().ok(),
                    };
                    match msg {
                        Some(msg) => {
                            let n = msg.len().min(cap);
                            unsafe {
                                core::ptr::copy_nonoverlapping(msg.as_ptr(), ptr, n);
                            }
                            out = SyscallResult::ok(n as u32);
                        }
                        None => {
                            out = SyscallResult::err(ERR_WOULD_BLOCK);
                        }
                    }
                }
            });
            if found { out } else { SyscallResult::err(ERR_NOT_FOUND) }
        }
        Syscall::IpcSend | Syscall::IpcRecv | Syscall::IpcReply => {
            // Get current task ID
            let task_id = CURRENT_TASK_ID.load(Ordering::Acquire) as u32;

            match Syscall::from_number(args.number) {
                Syscall::IpcSend => {
                    // arg0 = endpoint name ptr, arg1 = endpoint name len
                    // arg2 = payload ptr, arg3 = payload len
                    handle_ipc_send(args, task_id)
                }
                Syscall::IpcRecv => {
                    // arg0 = endpoint name ptr, arg1 = endpoint name len
                    // arg2 = buffer ptr, arg3 = buffer capacity
                    handle_ipc_recv(args, task_id)
                }
                Syscall::IpcReply => {
                    // arg0 = buffer ptr, arg1 = buffer len
                    handle_ipc_reply(args, task_id)
                }
                _ => SyscallResult::err(ERR_NOT_INIT),
            }
        }
        Syscall::NsPublish => ns_publish(args),
        Syscall::NsSubscribe => ns_subscribe(args),
        _ => SyscallResult::err(1),
    }
}

/// Publish a message on a cross-namespace endpoint.
///
/// `arg0` = pid (sender), `arg1` = from_ns, `arg2` = endpoint_name
/// address, `arg3` = endpoint_name length.
fn ns_publish(args: SyscallArgs) -> SyscallResult {
    use crate::ipc_namespace::EndpointName;
    use crate::kernel::with_kernel;
    let pid = crate::process::ProcessId(args.arg0);
    let from_ns = crate::ipc_namespace::NamespaceId(args.arg1);
    let name_ptr = args.arg2 as *const u8;
    let name_len = args.arg3 as usize;
    if name_ptr.is_null() || name_len == 0 || name_len > 256 {
        return SyscallResult::err(ERR_BAD_ARG);
    }
    let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
    let name_str = match core::str::from_utf8(name_bytes) {
        Ok(s) => s,
        Err(_) => return SyscallResult::err(ERR_BAD_ARG),
    };
    let name = EndpointName(alloc::string::String::from(name_str));
    let outcome = with_kernel(|k| {
        // Resolve the endpoint.
        let resolved = crate::ipc::resolve_endpoint(&k.namespaces, from_ns, &name);
        let (target_ns, _handle) = match resolved {
            Some(r) => r,
            None => return crate::ipc::DeliveryOutcome::UnknownEndpoint,
        };
        let target = match k.namespaces.get(target_ns) {
            Some(ns) => ns,
            None => return crate::ipc::DeliveryOutcome::UnknownEndpoint,
        };
        // Permission check.
        if !crate::ipc_namespace::may_publish_into(target, pid, from_ns, false) {
            return crate::ipc::DeliveryOutcome::Denied;
        }
        crate::ipc::DeliveryOutcome::Delivered
    });
    let r = outcome.unwrap_or(crate::ipc::DeliveryOutcome::UnknownEndpoint);
    let code: u32 = match r {
        crate::ipc::DeliveryOutcome::Delivered => 0,
        crate::ipc::DeliveryOutcome::NoSubscribers => 1,
        crate::ipc::DeliveryOutcome::Denied => 2,
        crate::ipc::DeliveryOutcome::UnknownEndpoint => 3,
    };
    SyscallResult::ok(code)
}

/// Subscribe to a cross-namespace endpoint. Currently a no-op; the
/// real implementation will register the calling process in
/// `namespace.allowed_subscribers`.
fn ns_subscribe(_args: SyscallArgs) -> SyscallResult {
    SyscallResult::ok(0)
}

/// Handle IPC Send: send a message to an endpoint
///
/// arg0 = endpoint name ptr, arg1 = endpoint name len
/// arg2 = payload ptr, arg3 = payload len
fn handle_ipc_send(args: SyscallArgs, sender_id: u32) -> SyscallResult {
    let name_ptr = args.arg0 as *const u8;
    let name_len = args.arg1 as usize;
    let payload_ptr = args.arg2 as *const u8;
    let payload_len = args.arg3 as usize;

    // Validate pointers
    if name_ptr.is_null() || name_len == 0 || name_len > 256 {
        return SyscallResult::err(ERR_BAD_ARG);
    }
    if payload_ptr.is_null() || payload_len == 0 {
        return SyscallResult::err(ERR_BAD_ARG);
    }
    if payload_len > MAX_PAYLOAD {
        return SyscallResult::err(ERR_PAYLOAD_TOO_BIG);
    }

    // Read endpoint name
    let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
    let name_str = match core::str::from_utf8(name_bytes) {
        Ok(s) => s,
        Err(_) => return SyscallResult::err(ERR_BAD_ARG),
    };
    let endpoint = EndpointName(alloc::string::String::from(name_str));

    // Read payload
    let payload = unsafe {
        core::slice::from_raw_parts(payload_ptr, payload_len)
    }.to_vec();

    // Look up endpoint and deliver message
    let delivered = IPC_ENDPOINTS.with(|endpoints| {
        let endpoint_entry = endpoints.get_mut(&endpoint);

        if let Some(ep) = endpoint_entry {
            // Generate sequence number
            static SEQ_COUNTER: AtomicUsize = AtomicUsize::new(0);
            let seq = SEQ_COUNTER.fetch_add(1, Ordering::Relaxed) as u64;

            // Create IPC message
            let msg = IpcMessage::new(
                ProcessId(sender_id),
                NamespaceId(0), // TODO: get from task context
                endpoint.clone(),
                seq,
                payload,
            );

            // Push to endpoint queue
            ep.messages.push_back(msg);
            true
        } else {
            false
        }
    });

    if delivered {
        // TODO: wake up any waiting receivers
        SyscallResult::ok(0)
    } else {
        SyscallResult::err(ERR_NOT_FOUND)
    }
}

/// Handle IPC Recv: receive a message from an endpoint
///
/// arg0 = endpoint name ptr, arg1 = endpoint name len
/// arg2 = buffer ptr, arg3 = buffer capacity
fn handle_ipc_recv(args: SyscallArgs, receiver_id: u32) -> SyscallResult {
    let name_ptr = args.arg0 as *const u8;
    let name_len = args.arg1 as usize;
    let buffer_ptr = args.arg2 as *mut u8;
    let buffer_cap = args.arg3 as usize;

    // Validate pointers
    if name_ptr.is_null() || name_len == 0 || name_len > 256 {
        return SyscallResult::err(ERR_BAD_ARG);
    }
    if buffer_ptr.is_null() || buffer_cap == 0 {
        return SyscallResult::err(ERR_BAD_ARG);
    }

    // Read endpoint name
    let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
    let name_str = match core::str::from_utf8(name_bytes) {
        Ok(s) => s,
        Err(_) => return SyscallResult::err(ERR_BAD_ARG),
    };
    let endpoint = EndpointName(alloc::string::String::from(name_str));

    // Look up endpoint and try to get message
    let result = IPC_ENDPOINTS.with(|endpoints| {
        let endpoint_entry = endpoints.get_mut(&endpoint);

        if let Some(ep) = endpoint_entry {
            if let Some(msg) = ep.messages.pop_front() {
                // Copy payload to user buffer
                let copy_len = msg.payload.len().min(buffer_cap);
                unsafe {
                    core::ptr::copy_nonoverlapping(msg.payload.as_ptr(), buffer_ptr, copy_len);
                }

                // Store reply context for potential reply
                let mut reply_ctx = CURRENT_REPLY.lock();
                *reply_ctx = Some(ReplyContext {
                    sender_id: msg.from.0,
                    endpoint: msg.endpoint.clone(),
                    seq: msg.seq,
                });

                // Return: bytes copied (high 16 bits) | sender PID (low 16 bits)
                SyscallResult::ok((copy_len as u32) << 16 | msg.from.0)
            } else {
                // No message available
                SyscallResult::err(ERR_QUEUE_EMPTY)
            }
        } else {
            SyscallResult::err(ERR_NOT_FOUND)
        }
    });

    result
}

/// Handle IPC Reply: send a reply back to the original sender
///
/// arg0 = buffer ptr, arg1 = buffer len
fn handle_ipc_reply(args: SyscallArgs, _receiver_id: u32) -> SyscallResult {
    let buffer_ptr = args.arg0 as *const u8;
    let buffer_len = args.arg1 as usize;

    if buffer_ptr.is_null() || buffer_len == 0 {
        return SyscallResult::err(ERR_BAD_ARG);
    }
    if buffer_len > MAX_PAYLOAD {
        return SyscallResult::err(ERR_PAYLOAD_TOO_BIG);
    }

    // Get the reply context
    let ctx = {
        let mut lock = CURRENT_REPLY.lock();
        lock.take()
    };

    match ctx {
        Some(context) => {
            // Read reply payload
            let payload = unsafe {
                core::slice::from_raw_parts(buffer_ptr, buffer_len)
            }.to_vec();

            // TODO: deliver reply to original sender
            // For now, we just acknowledge the reply was sent
            // A full implementation would wake up the sender's blocked receive

            // Clear reply context
            SyscallResult::ok(buffer_len as u32)
        }
        None => SyscallResult::err(ERR_NO_REPLY_CONTEXT),
    }
}

/// Wide-arg version for 64-bit host tests (pointer-safe).
pub fn handle_ipc_syscall_wide(args: WideSyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::ChannelCreate => {
            let cap = args.arg0;
            let id = NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed);
            let entry = if cap == 0 {
                let (s, r) = Channel::<Vec<u8>>::unbounded();
                ChannelEntry::Async(AsyncEntry { sender: s, receiver: r })
            } else {
                let (s, r) = SyncChannel::<Vec<u8>>::new(cap as usize);
                ChannelEntry::Bounded(SyncEntry { sender: s, receiver: r })
            };
            CHANNELS.with(|t| {
                t.insert(id, entry);
            });
            SyscallResult::ok(id)
        }
        Syscall::ChannelSend => {
            let cid = args.arg0;
            let ptr = args.arg1_ptr::<u8>();
            let len = args.arg2_lo as usize;
            if ptr.is_null() || len == 0 {
                return SyscallResult::err(ERR_BAD_ARG);
            }
            let bytes = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            let mut out = SyscallResult::err(ERR_WOULD_BLOCK);
            let mut found = false;
            CHANNELS.with(|t| {
                if let Some(entry) = t.get(&cid) {
                    found = true;
                    let r = match entry {
                        ChannelEntry::Async(e) => e.sender.try_send(bytes).map_err(|_e| ()),
                        ChannelEntry::Bounded(e) => e.sender.try_send(bytes).map_err(|_e| ()),
                    };
                    out = match r {
                        Ok(()) => SyscallResult::ok(len as u32),
                        Err(_) => SyscallResult::err(ERR_WOULD_BLOCK),
                    };
                }
            });
            if found { out } else { SyscallResult::err(ERR_NOT_FOUND) }
        }
        Syscall::ChannelRecv => {
            let cid = args.arg0;
            let ptr = args.arg2_ptr::<u8>();
            let cap = args.arg3 as usize;
            if ptr.is_null() || cap == 0 {
                return SyscallResult::err(ERR_BAD_ARG);
            }
            let mut out = SyscallResult::err(ERR_WOULD_BLOCK);
            let mut found = false;
            CHANNELS.with(|t| {
                if let Some(entry) = t.get(&cid) {
                    found = true;
                    let msg = match entry {
                        ChannelEntry::Async(e) => e.receiver.try_recv().ok(),
                        ChannelEntry::Bounded(e) => e.receiver.try_recv().ok(),
                    };
                    match msg {
                        Some(msg) => {
                            let n = msg.len().min(cap);
                            unsafe {
                                core::ptr::copy_nonoverlapping(msg.as_ptr(), ptr, n);
                            }
                            out = SyscallResult::ok(n as u32);
                        }
                        None => {
                            out = SyscallResult::err(ERR_WOULD_BLOCK);
                        }
                    }
                }
            });
            if found { out } else { SyscallResult::err(ERR_NOT_FOUND) }
        }
        _ => {
            // Fall back to narrow dispatch (truncating high bits).
            let narrow = SyscallArgs::new(args.number, args.arg0, args.arg1_lo, args.arg2_lo, args.arg3);
            handle_ipc_syscall(narrow)
        }
    }
}
