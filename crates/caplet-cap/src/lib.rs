//! CapletRT Capability Primitives
//!
//! This crate implements the core capability model for CapletRT.
//! It provides `Cap<T>`, `Handle<T>`, `CapSet`, and CSpace implementations.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

extern crate alloc;

pub mod attenuation;
pub mod cap;
pub mod cap_builder;
pub mod cap_set;
pub mod cspace;
pub mod derivation;
pub mod endpoint;
pub mod epoch;
pub mod error;
pub mod handle;
pub mod parent_link;
pub mod permissions;
pub mod revocation;
pub mod rights;
pub mod safety_bits;
pub mod safety_policy;

pub use cap::Cap;
pub use cap_builder::CapBuilder;
pub use cap_set::{AnyCap, CapKey, CapSet};
pub use cspace::CSpace;
pub use endpoint::{EndpointCapability, EndpointRights};
pub use epoch::Epoch;
pub use error::{CapError, CapResult};
pub use handle::Handle;
pub use permissions::Permissions;
pub use rights::{MemoryRights, RightsFilter};
pub use safety_bits::SafetyBits;
