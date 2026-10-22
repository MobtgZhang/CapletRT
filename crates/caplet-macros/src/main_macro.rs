//! Main macro implementation
//!
//! The `#[caplet::main]` attribute turns a function into the kernel
//! entry point.

use proc_macro::TokenStream as ProcTokenStream;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type};

/// Runtime path emitted into generated code.
///
/// The `caplet` facade re-exports `caplet-macros-exports` as
/// `caplet::runtime`, so a board only needs a dependency on the facade.
fn runtime_path() -> TokenStream {
    quote! { ::caplet::runtime }
}

/// Implements the `#[caplet::main]` attribute macro
pub fn main_impl(attr: ProcTokenStream, item: ProcTokenStream) -> ProcTokenStream {
    if !attr.is_empty() {
        return ProcTokenStream::from(
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "`#[caplet::main]` does not accept arguments",
            )
            .to_compile_error(),
        );
    }

    let input = syn::parse_macro_input!(item as ItemFn);
    match expand_main(input) {
        Ok(expanded) => ProcTokenStream::from(expanded),
        Err(error) => ProcTokenStream::from(error.to_compile_error()),
    }
}

/// Expand a validated `main` item. Kept separate from the proc-macro
/// entry point so the expansion and its diagnostics can be unit-tested
/// on the host.
fn expand_main(input: ItemFn) -> syn::Result<TokenStream> {
    validate_main(&input)?;

    let user_ident = &input.sig.ident;
    let runtime = runtime_path();
    let is_async = input.sig.asyncness.is_some();
    let arg_count = input.sig.inputs.len();

    let boot = if is_async && arg_count == 0 {
        quote! {
            #runtime::spawn_main_with(|_spawner, _caps| async move {
                #user_ident().await;
            });
        }
    } else if is_async {
        quote! {
            #runtime::spawn_main_with(|spawner, caps| async move {
                #user_ident(spawner, caps).await;
            });
        }
    } else {
        quote! {
            #runtime::spawn_main_with(|_spawner, _caps| async move {
                #user_ident();
            });
        }
    };

    Ok(quote! {
        #input

        #[doc(hidden)]
        #[no_mangle]
        pub extern "C" fn caplet_kernel_entry() -> ! {
            #runtime::kernel_init();
            #boot
            #runtime::kernel_start()
        }
    })
}

fn validate_main(input: &ItemFn) -> syn::Result<()> {
    let sig = &input.sig;
    if !sig.generics.params.is_empty() || sig.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &sig.generics,
            "`#[caplet::main]` cannot be applied to a generic function",
        ));
    }
    if sig.variadic.is_some() {
        return Err(syn::Error::new_spanned(
            &sig.variadic,
            "`#[caplet::main]` does not support variadic arguments",
        ));
    }

    let is_async = sig.asyncness.is_some();
    match sig.inputs.len() {
        0 => {
            if is_async {
                require_unit_return(&sig.output, "async main")?;
            } else {
                require_never_or_unit_return(&sig.output)?;
            }
        }
        2 if is_async => {
            require_unit_return(&sig.output, "async main")?;
            let mut args = sig.inputs.iter();
            let first = args.next().expect("length checked above");
            let second = args.next().expect("length checked above");
            require_arg_type(first, "Spawner")?;
            require_arg_type(second, "Caplets")?;
        }
        2 => {
            return Err(syn::Error::new_spanned(
                &sig.inputs,
                "a synchronous `#[caplet::main]` must take no arguments; use an async main to receive `Spawner` and `Caplets`",
            ));
        }
        _ => {
            return Err(syn::Error::new_spanned(
                &sig.inputs,
                "`#[caplet::main]` accepts either no arguments or `(Spawner, Caplets)` in an async main",
            ));
        }
    }
    Ok(())
}

fn require_arg_type(arg: &FnArg, expected: &str) -> syn::Result<()> {
    let FnArg::Typed(typed) = arg else {
        return Err(syn::Error::new_spanned(
            arg,
            "`#[caplet::main]` cannot take a receiver",
        ));
    };
    if !matches!(&*typed.pat, Pat::Ident(_)) {
        return Err(syn::Error::new_spanned(
            &typed.pat,
            "`#[caplet::main]` parameters must use identifier patterns",
        ));
    }
    let matches = match &*typed.ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident == expected)
            .unwrap_or(false),
        _ => false,
    };
    if !matches {
        return Err(syn::Error::new_spanned(
            &typed.ty,
            format!("expected `{expected}`"),
        ));
    }
    Ok(())
}

fn require_unit_return(output: &ReturnType, context: &str) -> syn::Result<()> {
    match output {
        ReturnType::Default => Ok(()),
        ReturnType::Type(_, ty) if matches!(&**ty, Type::Tuple(tuple) if tuple.elems.is_empty()) => {
            Ok(())
        }
        ReturnType::Type(_, ty) => Err(syn::Error::new_spanned(
            ty,
            format!("{context} must return `()`"),
        )),
    }
}

fn require_never_or_unit_return(output: &ReturnType) -> syn::Result<()> {
    match output {
        ReturnType::Default => Ok(()),
        ReturnType::Type(_, ty) => {
            let is_never = matches!(&**ty, Type::Never(_));
            let is_unit = matches!(&**ty, Type::Tuple(tuple) if tuple.elems.is_empty());
            if is_never || is_unit {
                Ok(())
            } else {
                Err(syn::Error::new_spanned(
                    ty,
                    "synchronous `#[caplet::main]` must return `()` or `!`",
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn async_main_receives_runtime_handles() {
        let item: ItemFn = syn::parse_quote! {
            async fn main(spawner: Spawner, caps: Caplets) {}
        };
        let expanded = expand_main(item).expect("valid async main").to_string();
        assert!(expanded.contains("spawn_main_with"));
        assert!(expanded.contains("main (spawner , caps) . await"));
        assert!(expanded.contains("caplet_kernel_entry"));
    }

    #[test]
    fn async_main_without_arguments_is_supported() {
        let item: ItemFn = syn::parse_quote! {
            async fn main() {}
        };
        let expanded = expand_main(item).expect("valid async main").to_string();
        assert!(expanded.contains("spawn_main_with"));
        assert!(expanded.contains("main () . await"));
    }

    #[test]
    fn sync_never_main_is_supported() {
        let item: ItemFn = syn::parse_quote! {
            fn main() -> ! { loop {} }
        };
        let expanded = expand_main(item).expect("valid sync main").to_string();
        assert!(expanded.contains("spawn_main_with"));
        assert!(expanded.contains("main ()"));
    }

    #[test]
    fn sync_main_rejects_runtime_handles() {
        let item: ItemFn = syn::parse_quote! {
            fn main(spawner: Spawner, caps: Caplets) {}
        };
        let error = expand_main(item).expect_err("sync arguments must be rejected");
        assert!(error.to_string().contains("must take no arguments"));
    }

    #[test]
    fn async_main_rejects_unknown_parameter() {
        let item: ItemFn = syn::parse_quote! {
            async fn main(foo: u32, caps: Caplets) {}
        };
        let error = expand_main(item).expect_err("parameter type must be checked");
        assert!(error.to_string().contains("expected `Spawner`"));
    }

    #[test]
    fn async_main_rejects_non_unit_return() {
        let item: ItemFn = syn::parse_quote! {
            async fn main() -> u32 { 1 }
        };
        let error = expand_main(item).expect_err("return type must be checked");
        assert!(error.to_string().contains("must return `()`"));
    }
}
