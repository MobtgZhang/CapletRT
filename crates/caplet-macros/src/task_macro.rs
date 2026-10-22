//! Task macro implementation
//!
//! The `#[caplet::task]` attribute accepts:
//!   - `priority = u8` (default 16)
//!   - `stack = usize` (default 4096)
//!   - `name = "..."` (default = function name)
//!   - `pool = "..."` (optional shared spawn pool)
//!   - `caps = [Uart1, ...]` (default empty)
//!   - `cpu_quota = u32` (default 10_000 us per epoch)
//!   - `mem_budget = usize` (default 1024 bytes)
//!   - `message_quota = u32` (default 100 messages per second)
//!   - `linkme` (opt-in; place the task in the link-time table)
//!
//! It generates:
//!   1. The original function under a hidden name.
//!   2. A re-export with the original name that forwards arguments.
//!   3. A `pub static __CAPLET_TASK_DESC_<NAME>: TaskDescriptor`.
//!   4. A `TaskRegistration` plus an explicit registration helper.
//!   5. A `__caplet_spawn_<NAME>(spawner)` helper.

extern crate alloc;

use proc_macro::TokenStream as ProcTokenStream;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Expr, FnArg, ItemFn, Lit, Pat, ReturnType, Type};

/// Task configuration parsed from attribute arguments.
#[derive(Debug, Clone, Default)]
struct TaskConfig {
    priority: Option<u8>,
    stack: Option<usize>,
    name: Option<String>,
    pool: Option<String>,
    caps: alloc::vec::Vec<String>,
    cpu_quota: Option<u32>,
    mem_budget: Option<usize>,
    message_quota: Option<u32>,
    linkme: bool,
}

/// Implements the `#[caplet::task]` attribute macro
pub fn task_impl(attr: ProcTokenStream, item: ProcTokenStream) -> ProcTokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let cfg = match parse_task_attrs(attr.into()) {
        Ok(cfg) => cfg,
        Err(error) => return ProcTokenStream::from(error.to_compile_error()),
    };

    match expand_task(input, cfg) {
        Ok(expanded) => ProcTokenStream::from(expanded),
        Err(error) => ProcTokenStream::from(error.to_compile_error()),
    }
}

fn expand_task(input: ItemFn, cfg: TaskConfig) -> syn::Result<TokenStream> {
    validate_task(&input)?;

    let fn_vis = &input.vis;
    let fn_block = &input.block;
    let fn_ident = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_attrs = &input.attrs;

    let priority = cfg.priority.unwrap_or(16);
    let stack = cfg.stack.unwrap_or(4096);
    let name = cfg.name.unwrap_or_else(|| fn_ident.to_string());
    let pool_lit = match &cfg.pool {
        Some(p) => quote! { ::core::option::Option::Some(#p) },
        None => quote! { ::core::option::Option::None },
    };
    let cap_literals: Vec<_> = cfg.caps.iter().map(|cap| quote! { #cap }).collect();
    let caps = quote! { &[#(#cap_literals),*] };
    let cpu_quota = cfg.cpu_quota.unwrap_or(10_000);
    let mem_budget = cfg.mem_budget.unwrap_or(1024);
    let message_quota = cfg.message_quota.unwrap_or(100);

    let wrapped_ident = format_ident!("__caplet_task_{}", fn_ident);
    let desc_ident = format_ident!("__CAPLET_TASK_DESC_{}", fn_ident);
    let spawn_ident = format_ident!("__caplet_spawn_{}", fn_ident);
    let entry_ident = format_ident!("__CAPLET_TASK_ENTRY_{}", fn_ident);
    let register_ident = format_ident!("__caplet_register_task_{}", fn_ident);
    let start_ident = format_ident!("__caplet_start_task_{}", fn_ident);
    let link_ident = format_ident!("__CAPLET_TASK_LINK_{}", fn_ident);

    // Extract argument patterns for forwarding.
    let arg_names: Vec<TokenStream> = fn_inputs
        .iter()
        .map(|arg| {
            let FnArg::Typed(typed) = arg else {
                unreachable!("validate_task rejects receivers");
            };
            let Pat::Ident(ident) = &*typed.pat else {
                unreachable!("validate_task rejects non-identifier patterns");
            };
            quote! { #ident }
        })
        .collect();
    let arg_types: Vec<_> = fn_inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(typed) => &typed.ty,
            FnArg::Receiver(_) => unreachable!("validate_task rejects receivers"),
        })
        .collect();
    let runtime = quote! { ::caplet::runtime };

    let (start_fn, start_value) = if arg_names.is_empty() {
        (
            quote! {
                #[doc(hidden)]
                pub fn #start_ident() {
                    let _ = #runtime::spawn_with_priority(#wrapped_ident(), #priority);
                }
            },
            quote! { ::core::option::Option::Some(#start_ident) },
        )
    } else {
        (quote! {}, quote! { ::core::option::Option::None })
    };

    let linkme_entry = if cfg.linkme {
        quote! {
            #[::caplet::runtime::task_table::distributed_slice(::caplet::runtime::task_table::LINKED_TASKS)]
            #[allow(non_upper_case_globals)]
            pub static #link_ident: #runtime::TaskRegistration =
                #runtime::TaskRegistration {
                    descriptor: &#desc_ident,
                    start: #start_value,
                };
        }
    } else {
        quote! {}
    };

    let mut hidden_sig = input.sig.clone();
    hidden_sig.ident = wrapped_ident.clone();
    let hidden_sig = &hidden_sig;

    Ok(quote! {
        #[doc(hidden)]
        #(#fn_attrs)*
        #fn_vis #hidden_sig #fn_block

        // Public re-export. We wrap the async inner function in a plain fn
        // that returns the future, so callers can pass `boot_task()`
        // (which evaluates to a future) directly to the executor's
        // spawn API.
        #fn_vis fn #fn_ident(#fn_inputs) -> impl ::core::future::Future<Output = ()> + Send + 'static {
            #wrapped_ident(#(#arg_names),*)
        }

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        pub static #desc_ident: #runtime::TaskDescriptor =
            #runtime::TaskDescriptor {
                name: #name,
                priority: #priority,
                stack: #stack,
                pool: #pool_lit,
                caps: #caps,
                cpu_quota: #cpu_quota,
                mem_budget: #mem_budget,
                message_quota: #message_quota,
            };

        #start_fn

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        pub static #entry_ident: #runtime::TaskRegistration =
            #runtime::TaskRegistration {
                descriptor: &#desc_ident,
                start: #start_value,
            };

        /// Explicit registration fallback for platforms where the
        /// link-time task section cannot be used.
        #[doc(hidden)]
        pub fn #register_ident<const N: usize>(table: &mut #runtime::TaskTable<N>) -> bool {
            table.register(&#entry_ident)
        }

        #linkme_entry

        #[doc(hidden)]
        pub fn #spawn_ident(
            spawner: &#runtime::Spawner,
            #(#arg_names: #arg_types),*
        ) -> #runtime::TaskHandle {
            spawner.spawn_with(
                #wrapped_ident(#(#arg_names),*),
                #priority,
                #stack,
            )
        }
    })
}

fn validate_task(input: &ItemFn) -> syn::Result<()> {
    let sig = &input.sig;
    if sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            sig.fn_token,
            "`#[caplet::task]` must be applied to an async function",
        ));
    }
    if !sig.generics.params.is_empty() || sig.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &sig.generics,
            "`#[caplet::task]` does not support generic tasks",
        ));
    }
    if sig.variadic.is_some() {
        return Err(syn::Error::new_spanned(
            &sig.variadic,
            "`#[caplet::task]` does not support variadic arguments",
        ));
    }
    if !matches!(sig.output, ReturnType::Default)
        && !matches!(&sig.output, ReturnType::Type(_, ty) if matches!(&**ty, Type::Tuple(tuple) if tuple.elems.is_empty()))
    {
        return Err(syn::Error::new_spanned(
            &sig.output,
            "`#[caplet::task]` tasks must return `()`",
        ));
    }
    for arg in &sig.inputs {
        let FnArg::Typed(typed) = arg else {
            return Err(syn::Error::new_spanned(
                arg,
                "`#[caplet::task]` cannot take a receiver",
            ));
        };
        if !matches!(&*typed.pat, Pat::Ident(_)) {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "task parameters must use identifier patterns so the spawn helper can forward them",
            ));
        }
    }
    Ok(())
}

fn parse_task_attrs(attr: TokenStream) -> syn::Result<TaskConfig> {
    let mut cfg = TaskConfig::default();
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("priority") {
            cfg.priority = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?);
        } else if meta.path.is_ident("stack") {
            cfg.stack = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?);
        } else if meta.path.is_ident("name") {
            cfg.name = Some(meta.value()?.parse::<syn::LitStr>()?.value());
        } else if meta.path.is_ident("pool") {
            cfg.pool = Some(meta.value()?.parse::<syn::LitStr>()?.value());
        } else if meta.path.is_ident("caps") {
            let array = meta.value()?.parse::<syn::ExprArray>()?;
            cfg.caps.clear();
            for expr in array.elems {
                cfg.caps.push(cap_name(&expr)?);
            }
        } else if meta.path.is_ident("cpu_quota") {
            cfg.cpu_quota = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?);
        } else if meta.path.is_ident("mem_budget") {
            cfg.mem_budget = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?);
        } else if meta.path.is_ident("message_quota") {
            cfg.message_quota = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?);
        } else if meta.path.is_ident("linkme") {
            cfg.linkme = true;
        } else {
            return Err(meta.error("unknown task attribute; expected priority, stack, name, pool, caps, cpu_quota, mem_budget, message_quota, or linkme"));
        }
        Ok(())
    });
    syn::parse::Parser::parse2(parser, attr)?;
    Ok(cfg)
}

/// Convert a capability-list element into the string stored in the
/// descriptor. Both `Uart1` and `"uart:1"` are accepted so board code
/// can use an identifier when the name is self-describing while still
/// allowing namespace-style service names.
fn cap_name(expr: &Expr) -> syn::Result<String> {
    match expr {
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .ok_or_else(|| syn::Error::new_spanned(expr, "empty capability name")),
        Expr::Lit(lit) => match &lit.lit {
            Lit::Str(value) => Ok(value.value()),
            _ => Err(syn::Error::new_spanned(
                expr,
                "capability entries must be identifiers or string literals",
            )),
        },
        _ => Err(syn::Error::new_spanned(
            expr,
            "capability entries must be identifiers or string literals",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn parses_quota_and_capability_fields() {
        let cfg = parse_task_attrs(quote! {
            priority = 3,
            stack = 1024,
            name = "sampler",
            pool = "pool-a",
            caps = [Uart1, "i2c:1/tmp102:0"],
            cpu_quota = 2500,
            mem_budget = 4096,
            message_quota = 32,
            linkme,
        })
        .expect("valid task attributes");

        assert_eq!(cfg.priority, Some(3));
        assert_eq!(cfg.stack, Some(1024));
        assert_eq!(cfg.name.as_deref(), Some("sampler"));
        assert_eq!(cfg.pool.as_deref(), Some("pool-a"));
        assert_eq!(cfg.caps, ["Uart1", "i2c:1/tmp102:0"]);
        assert_eq!(cfg.cpu_quota, Some(2500));
        assert_eq!(cfg.mem_budget, Some(4096));
        assert_eq!(cfg.message_quota, Some(32));
        assert!(cfg.linkme);
    }

    #[test]
    fn empty_attributes_use_budget_defaults() {
        let cfg = parse_task_attrs(quote! {}).expect("empty attributes are valid");
        assert_eq!(cfg.priority, None);
        assert_eq!(cfg.stack, None);
        assert!(cfg.caps.is_empty());
        assert_eq!(cfg.cpu_quota, None);
        assert_eq!(cfg.mem_budget, None);
        assert_eq!(cfg.message_quota, None);
    }

    #[test]
    fn rejects_unknown_attribute() {
        let error = parse_task_attrs(quote! { priorities = 3 }).expect_err("typo must be rejected");
        assert!(error.to_string().contains("unknown task attribute"));
    }

    #[test]
    fn expansion_forwards_arguments_in_spawn_helper() {
        let input: ItemFn = syn::parse_quote! {
            async fn worker(device: u32, count: usize) {}
        };
        let expanded = expand_task(input, TaskConfig::default())
            .expect("valid task")
            .to_string();
        assert!(expanded.contains("__caplet_spawn_worker"));
        assert!(expanded.contains("device : u32"));
        assert!(expanded.contains("count : usize"));
        assert!(expanded.contains("__caplet_task_worker (device , count)"));
    }

    #[test]
    fn rejects_sync_task() {
        let input: ItemFn = syn::parse_quote! {
            fn worker() {}
        };
        let error =
            expand_task(input, TaskConfig::default()).expect_err("sync tasks must be rejected");
        assert!(error.to_string().contains("async function"));
    }

    #[test]
    fn rejects_non_unit_task_return() {
        let input: ItemFn = syn::parse_quote! {
            async fn worker() -> u32 { 1 }
        };
        let error =
            expand_task(input, TaskConfig::default()).expect_err("non-unit tasks must be rejected");
        assert!(error.to_string().contains("must return `()`"));
    }
}
