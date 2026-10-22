//! CapletRT network stack
//!
//! Capability-mediated, pluggable, protocol-agnostic. Provides:
//!
//! * [`NetworkInterface`] — the hardware abstraction boundary
//! * [`MacAddr`] / [`Ipv4Addr`] / etc. — value types
//! * [`Socket`] — capability-protected socket descriptor
//! * [`syscall_net`] — entry point invoked by the kernel's
//!   syscall dispatch table

#![no_std]
#![deny(missing_docs)]
// MSRV 1.75 predates `slice::as_chunks`; keep the portable form.
#![allow(clippy::chunks_exact_to_as_chunks)]

extern crate alloc;

pub mod addr;
pub mod arp;
pub mod cap;
pub mod dhcp;
pub mod dns;
pub mod eth;
pub mod icmp;
pub mod interface;
pub mod proto;
pub mod socket;
pub mod syscall;
pub mod tcp;

pub use addr::{Ipv4Addr, MacAddr, Port};
pub use interface::{NetError, NetworkInterface, RxPacket, TxPacket};
pub use socket::{Socket, SocketError, SocketKind};
pub use syscall::{syscall_net, NetSyscall};
