//! Cross-namespace IPC.
//!
//! This module hosts the message types and helper functions used by
//! the cross-namespace IPC syscalls (`ns_publish`, `ns_subscribe`).
//! The actual dispatch logic is in [`crate::syscall_ipc`]; this file
//! only provides the data model.
//!
//! docv1.md §5.5 / §7.5 promises that an `Endpoint` is a
//! first-class capability. Namespaces refine the visibility of
//! endpoints; the type system here mirrors that.

extern crate alloc;
use alloc::vec::Vec;

use crate::ipc_namespace::{EndpointName, NamespaceId, NamespaceTree};
use crate::process::ProcessId;

/// Maximum payload of a single IPC message in M3 (4 KiB to match
/// a typical page). Configurable per-endpoint at registration
/// time.
pub const MAX_IPC_PAYLOAD: usize = 4096;

/// An IPC message as it sits in the kernel's queue.
#[derive(Debug, Clone)]
pub struct IpcMessage {
    /// Sender process id.
    pub from: ProcessId,
    /// Namespace in which the sender published.
    pub from_ns: NamespaceId,
    /// Symbolic endpoint name.
    pub endpoint: EndpointName,
    /// Monotonically increasing message id within `(from, endpoint)`.
    pub seq: u64,
    /// Payload bytes. The kernel copies these into the receiver's
    /// CSpace-allocated buffer; userspace never holds kernel
    /// pointers.
    pub payload: Vec<u8>,
}

impl IpcMessage {
    /// Construct a new message.
    pub fn new(
        from: ProcessId,
        from_ns: NamespaceId,
        endpoint: EndpointName,
        seq: u64,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            from,
            from_ns,
            endpoint,
            seq,
            payload,
        }
    }

    /// Returns `true` if the payload size is within the M3 limit.
    pub fn fits(&self) -> bool {
        self.payload.len() <= MAX_IPC_PAYLOAD
    }
}

/// Delivery outcome of an `ns_publish` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// Message was delivered to at least one subscriber.
    Delivered,
    /// No subscribers were listening in the resolved namespace.
    NoSubscribers,
    /// The resolved namespace explicitly refused the publish.
    Denied,
    /// The endpoint was not found in the namespace tree.
    UnknownEndpoint,
}

/// Resolve an endpoint name in a namespace tree and return the
/// matching namespace id and handle, if any.
pub fn resolve_endpoint(
    tree: &NamespaceTree,
    start: NamespaceId,
    name: &EndpointName,
) -> Option<(NamespaceId, u32)> {
    tree.resolve(start, name).map(|(id, h)| (id, h.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_fits_within_limit() {
        let m = IpcMessage::new(
            ProcessId(1),
            NamespaceId(0),
            EndpointName::from_static("svc"),
            0,
            alloc::vec![0u8; 100],
        );
        assert!(m.fits());
    }

    #[test]
    fn message_rejects_oversize() {
        let m = IpcMessage::new(
            ProcessId(1),
            NamespaceId(0),
            EndpointName::from_static("svc"),
            0,
            alloc::vec![0u8; MAX_IPC_PAYLOAD + 1],
        );
        assert!(!m.fits());
    }

    #[test]
    fn delivery_outcome_variants() {
        assert_eq!(DeliveryOutcome::Delivered, DeliveryOutcome::Delivered);
        assert_ne!(DeliveryOutcome::Delivered, DeliveryOutcome::Denied);
    }
}
