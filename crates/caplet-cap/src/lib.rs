//! CapletRT Capability Primitives
//!
//! This crate implements the core capability model for CapletRT.
//! It provides `Cap<T>`, `Handle<T>`, `CapSet`, and CSpace implementations.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

extern crate alloc;

pub mod error;
pub mod handle;
pub mod cap;
pub mod cap_set;
pub mod cap_builder;
pub mod permissions;
pub mod rights;
pub mod attenuation;
pub mod safety_bits;
pub mod safety_policy;
pub mod derivation;
pub mod revocation;
pub mod epoch;
pub mod parent_link;
pub mod cspace;
pub mod endpoint;

pub use error::{CapError, CapResult};
pub use handle::Handle;
pub use cap::Cap;
pub use cap_set::{CapSet, CapKey, AnyCap};
pub use cap_builder::CapBuilder;
pub use permissions::Permissions;
pub use rights::{RightsFilter, MemoryRights};
pub use safety_bits::SafetyBits;
pub use epoch::Epoch;
pub use cspace::CSpace;
pub use endpoint::{EndpointCapability, EndpointRights};