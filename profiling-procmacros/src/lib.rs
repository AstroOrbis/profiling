extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, parse_quote, ImplItem, ItemFn, ItemImpl};

/// Intended to be used at the crate root to profile all functions / impl blocks in the crate.
#[proc_macro_attribute]
pub fn everything(
    args: TokenStream,
    input: TokenStream,
) -> TokenStream {
    if !args.is_empty() {
        panic!("`#![profiling::everything]` does not take any arguments")
    }

    let mut file = parse_macro_input!(input as syn::File);

    for item in &mut file.items {
        match item {
            syn::Item::Fn(func) => {
                // Skip const functions and those explicitly marked #[profiling::skip]
                let mut skip_this = func.sig.constness.is_some();
                for attr in &func.attrs {
                    let path = attr.path();
                    if path.segments.last().map(|s| s.ident.to_string()) == Some("skip".to_string())
                    {
                        skip_this = true;
                        break;
                    }
                }
                if skip_this {
                    continue;
                }

                let func_name = func.sig.ident.to_string();
                let prev_block = &func.block;
                func.block = Box::new(impl_block(prev_block, &func_name));
            }

            syn::Item::Impl(item_impl) => {
                let struct_name = item_impl.self_ty.to_token_stream().to_string();
                for impl_item in &mut item_impl.items {
                    let ImplItem::Fn(ref mut func) = impl_item else {
                        continue;
                    };

                    let mut skip_this = func.sig.constness.is_some();
                    for attr in &func.attrs {
                        let path = attr.path();
                        if path.segments.last().map(|s| s.ident.to_string())
                            == Some("skip".to_string())
                        {
                            skip_this = true;
                            break;
                        }
                    }
                    if skip_this {
                        continue;
                    }

                    let calling_info = format!("{}: {}", struct_name, func.sig.ident);
                    let prev_block = &func.block;
                    func.block = impl_block(prev_block, &calling_info);
                }
            }

            _ => {}
        }
    }

    (quote! { #file }).into()
}

#[proc_macro_attribute]
pub fn function(
    _attr: TokenStream,
    item: TokenStream,
) -> TokenStream {
    let mut function = parse_macro_input!(item as ItemFn);
    let instrumented_function_name = function.sig.ident.to_string();

    let body = &function.block;
    let new_body: syn::Block = impl_block(body, &instrumented_function_name);

    function.block = Box::new(new_body);

    (quote! {
        #function
    })
    .into()
}

#[proc_macro_attribute]
pub fn skip(
    _attr: TokenStream,
    item: TokenStream,
) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn all_functions(
    _attr: TokenStream,
    item: TokenStream,
) -> TokenStream {
    let mut content = parse_macro_input!(item as ItemImpl);
    let struct_name = content.self_ty.to_token_stream().to_string();

    'func_loop: for block in &mut content.items {
        // Currently, we only care about the function impl part.
        // In the future, expand the code to following if we are interested in other parts
        //
        // match block {
        //     ImplItem::Fn(ref mut func) => {
        //         for func_attr in &func.attrs {
        //             if let syn::Meta::Path(ref func_attr_info) = func_attr.meta {
        //                 let attr_seg = func_attr_info.segments.last().unwrap();
        //                 if attr_seg.ident.to_string() == "skip".to_string() {
        //                     continue 'func_loop;
        //                 }
        //             }
        //         }
        //         let prev_block = &func.block;
        //         let func_name = func.sig.ident.to_string();
        //         func.block = impl_block(prev_block, &func_name);
        //     }
        //     ImplItem::Macro(_) => { // some code... },
        //     ImplItem::Type(_) => { // some code... },
        //     _ => {}
        // }
        let ImplItem::Fn(ref mut func) = block else {
            continue;
        };

        for func_attr in &func.attrs {
            let func_attr_info = func_attr.path();
            if func_attr_info.segments.is_empty() {
                continue;
            }
            if func_attr_info.segments.first().unwrap().ident != "profiling" {
                continue;
            }
            if func_attr_info.segments.last().unwrap().ident == "skip" {
                continue 'func_loop;
            }
        }

        // Skip const functions
        if func.sig.constness.is_some() {
            continue 'func_loop;
        }

        let prev_block = &func.block;
        let calling_info = format!("{}: {}", struct_name, func.sig.ident);
        func.block = impl_block(prev_block, &calling_info);
    }

    (quote!(
        #content
    ))
    .into()
}

#[cfg(not(any(
    feature = "profile-with-puffin",
    feature = "profile-with-optick",
    feature = "profile-with-superluminal",
    feature = "profile-with-tracing",
    feature = "profile-with-tracy"
)))]
fn impl_block(
    body: &syn::Block,
    _instrumented_function_name: &str,
) -> syn::Block {
    parse_quote! {
        {
            #body
        }
    }
}

#[cfg(any(
    feature = "profile-with-puffin",
    feature = "profile-with-optick",
    feature = "profile-with-superluminal",
    feature = "profile-with-tracy"
))]
fn impl_block(
    body: &syn::Block,
    _instrumented_function_name: &str,
) -> syn::Block {
    parse_quote! {
        {
            profiling::function_scope!();

            #body
        }
    }
}

#[cfg(feature = "profile-with-tracing")]
fn impl_block(
    body: &syn::Block,
    instrumented_function_name: &str,
) -> syn::Block {
    parse_quote! {
        {
            let _fn_span = profiling::tracing::span!(profiling::tracing::Level::INFO, #instrumented_function_name);
            let _fn_span_entered = _fn_span.enter();

            #body
        }
    }
}
