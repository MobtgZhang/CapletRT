//! Tiny bridge that lets the Cortex-M port install hooks into the
//! kernel without creating a circular dependency. The kernel exposes
//! `set_idle_hook` (a function pointer); this module re-exports it
//! so the port can call it without a `use caplet_kernel::...`
//! statement that would cycle the dependency graph.

pub use caplet_kernel::tickless::set_idle_hook;