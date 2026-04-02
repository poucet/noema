use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::parse::{ParsedMethod, ParsedTrait, ReturnKind, RpcKind};

/// Generate the `XxxApiService<T>` struct + `RpcService` impl + metadata constant.
pub fn generate(parsed: &ParsedTrait) -> syn::Result<TokenStream> {
    let service_name = parsed.service_name();
    let trait_name = &parsed.trait_name;
    let prefix = &parsed.prefix;
    let vis = &parsed.vis;
    let meta_const_name = parsed.meta_const_name();

    let rpc_methods: Vec<_> = parsed
        .methods
        .iter()
        .filter(|m| m.rpc_kind != RpcKind::Skip)
        .collect();

    let match_arms: Vec<TokenStream> = rpc_methods
        .iter()
        .map(|m| generate_dispatch_arm(m))
        .collect::<syn::Result<Vec<_>>>()?;

    let stream_type = find_stream_type(&rpc_methods);

    // Generate method metadata entries
    let method_metas: Vec<TokenStream> = rpc_methods
        .iter()
        .map(|m| {
            let name = &m.method_name;
            let hash = signature_hash(m);
            quote! {
                ::simply_rpc::MethodMeta { name: #name, signature_hash: #hash }
            }
        })
        .collect();

    Ok(quote! {
        /// Auto-generated RPC service wrapper (internal — use `TraitName::service()` instead).
        #[doc(hidden)]
        #vis struct #service_name<T: #trait_name + ?Sized>(::std::sync::Arc<T>);

        /// Auto-generated method metadata for compatibility checking.
        #vis const #meta_const_name: ::simply_rpc::ServiceMeta = ::simply_rpc::ServiceMeta {
            prefix: #prefix,
            methods: &[#(#method_metas),*],
        };

        impl dyn #trait_name {
            /// Create an RPC service from an `Arc<dyn Trait>`.
            pub fn service(arc: ::std::sync::Arc<dyn #trait_name>) -> ::std::sync::Arc<#service_name<dyn #trait_name>> {
                ::std::sync::Arc::new(#service_name(arc))
            }

            /// Get the service metadata for compatibility checking.
            pub fn meta() -> &'static ::simply_rpc::ServiceMeta {
                &#meta_const_name
            }
        }

        #[::async_trait::async_trait]
        impl<T: #trait_name + ?Sized + 'static> ::simply_rpc::RpcService for #service_name<T> {
            type Stream = #stream_type;

            fn prefix(&self) -> &str {
                #prefix
            }

            fn meta(&self) -> &'static ::simply_rpc::ServiceMeta {
                &#meta_const_name
            }

            async fn dispatch(
                &self,
                method: &str,
                params: ::serde_json::Value,
            ) -> Option<::simply_rpc::DispatchResult<Self::Stream>> {
                match method {
                    #(#match_arms)*
                    _ => None,
                }
            }
        }
    })
}

/// Hash a method's full signature (params + return) for compatibility checking.
fn signature_hash(method: &ParsedMethod) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Hash param types (owned forms, since that's what goes over the wire)
    for p in &method.params {
        let owned = &p.owned_type;
        let ty_str = quote! { #owned }.to_string();
        ty_str.hash(&mut hasher);
    }
    // Hash return kind + type
    let ret_str = match &method.return_kind {
        ReturnKind::ResultUnit => "Result<()>".to_string(),
        ReturnKind::ResultValue { inner } => format!("Result<{}>", quote! { #inner }),
        ReturnKind::RawValue { inner } => quote! { #inner }.to_string(),
        ReturnKind::StreamTuple { value_type, stream_type } => {
            format!("Result<({}, {})>", quote! { #value_type }, quote! { #stream_type })
        }
        ReturnKind::StreamBare { stream_type } => {
            format!("Result<{}>", quote! { #stream_type })
        }
    };
    ret_str.hash(&mut hasher);
    hasher.finish()
}

/// Find the stream type from stream methods, or default to `()`.
fn find_stream_type(methods: &[&ParsedMethod]) -> TokenStream {
    for m in methods {
        match &m.return_kind {
            ReturnKind::StreamTuple { stream_type, .. }
            | ReturnKind::StreamBare { stream_type } => {
                return quote! { #stream_type };
            }
            _ => {}
        }
    }
    quote! { () }
}

/// Generate a single match arm for a method.
fn generate_dispatch_arm(method: &ParsedMethod) -> syn::Result<TokenStream> {
    let method_str = &method.method_name;

    let (deser, call_args) = generate_deser_and_args(method);
    let body = generate_dispatch_body(method, &call_args);

    Ok(quote! {
        #method_str => {
            #deser
            #body
        }
    })
}

/// Generate deserialization code and the arguments to pass to the trait method.
fn generate_deser_and_args(method: &ParsedMethod) -> (TokenStream, Vec<TokenStream>) {
    let params = &method.params;

    if params.is_empty() {
        return (quote! {}, vec![]);
    }

    if params.len() == 1 {
        let p = &params[0];
        let name = &p.name;
        let owned_type = &p.owned_type;
        let name_str = name.to_string();

        let deser = if method.is_base64_param(&name_str) {
            // base64: deserialize as String, decode to Vec<u8>
            quote! {
                let __b64: String = match ::serde_json::from_value(params) {
                    Ok(v) => v,
                    Err(e) => return Some(::simply_rpc::DispatchResult::value(
                        Err(::anyhow::anyhow!("deserialize error: {}", e))
                    )),
                };
                let #name: Vec<u8> = match ::simply_rpc::decode_base64(&__b64) {
                    Ok(v) => v,
                    Err(e) => return Some(::simply_rpc::DispatchResult::value(Err(e))),
                };
            }
        } else {
            quote! {
                let #name: #owned_type = match ::serde_json::from_value(params) {
                    Ok(v) => v,
                    Err(e) => return Some(::simply_rpc::DispatchResult::value(
                        Err(::anyhow::anyhow!("deserialize error: {}", e))
                    )),
                };
            }
        };

        let arg = if p.is_ref || p.is_str_ref {
            quote! { &#name }
        } else {
            quote! { #name }
        };

        return (deser, vec![arg]);
    }

    // Multi-param: generate a Params struct
    // base64_param fields become String on the wire
    let struct_name = format_ident!("__RpcParams_{}", method.name);
    let fields: Vec<TokenStream> = params
        .iter()
        .map(|p| {
            let name = &p.name;
            if method.is_base64_param(&name.to_string()) {
                quote! { #name: String }
            } else {
                let owned_type = &p.owned_type;
                quote! { #name: #owned_type }
            }
        })
        .collect();

    let deser = quote! {
        #[derive(::serde::Deserialize)]
        struct #struct_name {
            #(#fields,)*
        }
        let __p: #struct_name = match ::serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Some(::simply_rpc::DispatchResult::value(
                Err(::anyhow::anyhow!("deserialize error: {}", e))
            )),
        };
    };

    let args: Vec<TokenStream> = params
        .iter()
        .map(|p| {
            let name = &p.name;
            if method.is_base64_param(&name.to_string()) {
                quote! { {
                    match ::simply_rpc::decode_base64(&__p.#name) {
                        Ok(v) => v,
                        Err(e) => return Some(::simply_rpc::DispatchResult::value(Err(e))),
                    }
                } }
            } else if p.is_ref || p.is_str_ref {
                quote! { &__p.#name }
            } else {
                quote! { __p.#name }
            }
        })
        .collect();

    (deser, args)
}

/// Check if a type is `Vec<u8>`.
fn is_vec_u8(ty: &syn::Type) -> bool {
    let s = quote! { #ty }.to_string().replace(' ', "");
    s == "Vec<u8>"
}

/// Generate the body that calls the trait method and wraps the result.
fn generate_dispatch_body(method: &ParsedMethod, call_args: &[TokenStream]) -> TokenStream {
    let fn_name = &method.name;
    let call = quote! { self.0.#fn_name(#(#call_args),*).await };

    // For base64_return methods returning Result<Vec<u8>>, encode the result
    if method.base64_return {
        if let ReturnKind::ResultValue { .. } = &method.return_kind {
            return quote! {
                Some(::simply_rpc::DispatchResult::value(match #call {
                    Ok(bytes) => Ok(::serde_json::Value::String(
                        ::simply_rpc::encode_base64(&bytes)
                    )),
                    Err(e) => Err(e),
                }))
            };
        }
    }

    match &method.return_kind {
        ReturnKind::ResultUnit => {
            quote! {
                Some(::simply_rpc::DispatchResult::value(
                    ::simply_rpc::call_unit(#call)
                ))
            }
        }
        ReturnKind::ResultValue { .. } => {
            quote! {
                Some(::simply_rpc::DispatchResult::value(
                    ::simply_rpc::call_val(#call)
                ))
            }
        }
        ReturnKind::RawValue { .. } => {
            quote! {
                Some(::simply_rpc::DispatchResult::value(
                    ::simply_rpc::call_raw(#call)
                ))
            }
        }
        ReturnKind::StreamTuple { .. } => {
            // Result<(T, S)> — serialize T as the result, return S as a stream
            quote! {
                Some(match #call {
                    Ok((__value, __stream)) => {
                        ::simply_rpc::DispatchResult::with_stream(
                            ::simply_rpc::call_val(Ok(__value)),
                            __stream,
                        )
                    }
                    Err(e) => ::simply_rpc::DispatchResult::value(Err(e)),
                })
            }
        }
        ReturnKind::StreamBare { .. } => {
            // Result<S> — return true as the result, S as a stream
            quote! {
                Some(match #call {
                    Ok(__stream) => {
                        ::simply_rpc::DispatchResult::with_stream(
                            Ok(::serde_json::Value::Bool(true)),
                            __stream,
                        )
                    }
                    Err(e) => ::simply_rpc::DispatchResult::value(Err(e)),
                })
            }
        }
    }
}
