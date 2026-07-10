//! Task macro implementation
//!
//! The `#[caplet::task]` attribute accepts:
//!   - `priority = u8` (default 16)
//!   - `stack = usize` (default 4096)
//!   - `name = "..."` (default = function name)
//!   - `pool = "..."` (optional shared spawn pool)
//!
//! It generates:
//!   1. The original function under a hidden name.
//!   2. A re-export with the original name that forwards arguments.
//!   3. A `pub static __CAPLET_TASK_DESC_<NAME>: TaskDescriptor`.
//!   4. A `__caplet_spawn_<NAME>(spawner)` helper.

extern crate alloc;

use proc_macro::TokenStream as ProcTokenStream;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, FnArg, ItemFn, Pat};

/// Task configuration parsed from attribute arguments.
#[derive(Debug, Clone, Default)]
struct TaskConfig {
    priority: Option<u8>,
    stack: Option<usize>,
    name: Option<String>,
    pool: Option<String>,
}

/// Implements the `#[caplet::task]` attribute macro
pub fn task_impl(attr: ProcTokenStream, item: ProcTokenStream) -> ProcTokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let cfg = parse_task_attrs(attr.into());

    let fn_vis = &input.vis;
    let fn_block = &input.block;
    let fn_ident = &input.sig.ident;
    let fn_async = &input.sig.asyncness;
    let fn_generics = &input.sig.generics;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_attrs = &input.attrs;

    let priority = cfg.priority.unwrap_or(16);
    let stack = cfg.stack.unwrap_or(4096);
    let name = cfg.name.unwrap_or_else(|| fn_ident.to_string());
    let pool_lit = match &cfg.pool {
        Some(p) => quote! { ::core::option::Option::Some(#p) },
        None => quote! { ::core::option::Option::None },
    };

    let wrapped_ident = format_ident!("__caplet_task_{}", fn_ident);
    let desc_ident = format_ident!("__CAPLET_TASK_DESC_{}", fn_ident);
    let spawn_ident = format_ident!("__caplet_spawn_{}", fn_ident);

    // Extract argument patterns for forwarding.
    let arg_names: Vec<TokenStream> = fn_inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pt) = arg {
                if let Pat::Ident(pi) = &*pt.pat {
                    return Some(quote! { #pi });
                }
            }
            None
        })
        .collect();

    let expanded = quote! {
        #[doc(hidden)]
        #(#fn_attrs)*
        #fn_vis #fn_async fn #wrapped_ident #fn_generics(#fn_inputs) #fn_output #fn_block

        // Public re-export. We wrap the async inner function in a plain fn
        // that returns the future, so callers can pass `boot_task()`
        // (which evaluates to a future) directly to the executor's
        // spawn API.
        #fn_vis fn #fn_ident #fn_generics(#fn_inputs) -> impl ::core::future::Future<Output = ()> + Send + 'static {
            #wrapped_ident(#(#arg_names),*)
        }

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        pub static #desc_ident: ::caplet_macros_exports::TaskDescriptor =
            ::caplet_macros_exports::TaskDescriptor {
                name: #name,
                priority: #priority,
                stack: #stack,
                pool: #pool_lit,
            };

        #[doc(hidden)]
        pub fn #spawn_ident(
            spawner: &::caplet_macros_exports::Spawner,
        ) -> ::caplet_macros_exports::TaskHandle {
            spawner.spawn_with(
                #wrapped_ident(#(#arg_names),*),
                #priority,
                #stack,
            )
        }
    };

    ProcTokenStream::from(expanded)
}

fn parse_task_attrs(attr: TokenStream) -> TaskConfig {
    let mut cfg = TaskConfig::default();
    let s = attr.to_string();
    for part in split_top_level(&s, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut iter = part.splitn(2, '=');
        let key = iter.next().unwrap_or("").trim();
        let val = iter.next().unwrap_or("").trim();
        match key {
            "priority" => cfg.priority = val.parse().ok(),
            "stack" => cfg.stack = val.parse().ok(),
            "name" => cfg.name = Some(val.trim_matches('"').to_string()),
            "pool" => cfg.pool = Some(val.trim_matches('"').to_string()),
            _ => {}
        }
    }
    cfg
}

/// Splits a string at the top-level commas (ignoring commas inside parens).
fn split_top_level(s: &str, sep: char) -> alloc::vec::Vec<&str> {
    let mut out = alloc::vec::Vec::new();
    let mut depth = 0usize;
    let mut last = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if c == sep && depth == 0 => {
                out.push(&s[last..i]);
                last = i + sep.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[last..]);
    out
}