//! Main macro implementation
//!
//! The `#[caplet::main]` attribute turns a function into the kernel
//! entry point.

use proc_macro::TokenStream as ProcTokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, Pat};

/// Implements the `#[caplet::main]` attribute macro
pub fn main_impl(_attr: ProcTokenStream, item: ProcTokenStream) -> ProcTokenStream {
    let input = syn::parse_macro_input!(item as ItemFn);

    let fn_vis = &input.vis;
    let fn_block = &input.block;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_attrs = &input.attrs;
    let user_ident = &input.sig.ident;
    let stashed = format_ident!("__caplet_user_main_{}", user_ident);

    let arg_names: Vec<_> = fn_inputs
        .iter()
        .filter_map(|a| {
            if let FnArg::Typed(pt) = a {
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
        #fn_vis fn #stashed(#fn_inputs) #fn_output #fn_block

        #fn_vis fn #user_ident(#fn_inputs) #fn_output {
            #stashed(#(#arg_names),*)
        }

        #[doc(hidden)]
        #[no_mangle]
        pub unsafe extern "C" fn caplet_kernel_entry() -> ! {
            ::caplet_macros_exports::kernel_init();
            ::caplet_macros_exports::spawn_main(#user_ident);
            ::caplet_macros_exports::kernel_start();
        }
    };

    ProcTokenStream::from(expanded)
}