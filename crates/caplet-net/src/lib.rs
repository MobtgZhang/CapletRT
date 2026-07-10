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

extern crate alloc;

pub mod addr;
pub mod interface;
pub mod socket;
pub mod syscall;
pub mod proto;
pub mod eth;
pub mod icmp;
pub mod tcp;
pub mod dhcp;
pub mod dns;
pub mod cap;

pub use addr::{Ipv4Addr, MacAddr, Port};
pub use interface::{NetworkInterface, RxPacket, TxPacket};
pub use socket::{Socket, SocketError, SocketKind};
pub use syscall::{syscall_net, NetSyscall};