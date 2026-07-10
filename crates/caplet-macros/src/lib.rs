//! CapletRT Procedural Macros

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod task_macro;
mod main_macro;

use proc_macro::TokenStream;

/// Task declaration macro
#[proc_macro_attribute]
pub fn task(attr: TokenStream, item: TokenStream) -> TokenStream {
    task_macro::task_impl(attr, item)
}

/// Main entry point macro
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    main_macro::main_impl(attr, item)
}

/// Alias so users can write `#[caplet::main]` instead of
/// `#[caplet_macros::main]`. The lib re-exports below provide
/// `caplet::main` and `caplet::task`.
#[proc_macro_attribute]
pub fn main_attr(attr: TokenStream, item: TokenStream) -> TokenStream {
    main_macro::main_impl(attr, item)
}