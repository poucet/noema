use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::parse::{HttpMethod, ParsedMethod, ParsedTrait, ReturnKind, RpcKind};

/// Convert a parse-time HttpMethod to the runtime RouteKind token.
fn route_kind_tokens(method: &HttpMethod) -> TokenStream {
    match method {
        HttpMethod::Get => quote! { ::simply_rpc::RouteKind::Rest(::simply_rpc::HttpMethod::Get) },
        HttpMethod::Post => quote! { ::simply_rpc::RouteKind::Rest(::simply_rpc::HttpMethod::Post) },
        HttpMethod::Put => quote! { ::simply_rpc::RouteKind::Rest(::simply_rpc::HttpMethod::Put) },
        HttpMethod::Delete => quote! { ::simply_rpc::RouteKind::Rest(::simply_rpc::HttpMethod::Delete) },
        HttpMethod::Stream => quote! { ::simply_rpc::RouteKind::Stream },
    }
}

/// Convert a parse-time HttpMethod to the runtime HttpMethod token (REST only).
fn http_method_tokens(method: &HttpMethod) -> TokenStream {
    match method {
        HttpMethod::Get => quote! { ::simply_rpc::HttpMethod::Get },
        HttpMethod::Post => quote! { ::simply_rpc::HttpMethod::Post },
        HttpMethod::Put => quote! { ::simply_rpc::HttpMethod::Put },
        HttpMethod::Delete => quote! { ::simply_rpc::HttpMethod::Delete },
        HttpMethod::Stream => panic!("Stream is not an HttpMethod"),
    }
}

/// Check if a return type is `BinaryResponse` (or `Result<BinaryResponse>`).
fn is_binary_response_type(return_kind: &ReturnKind) -> bool {
    match return_kind {
        ReturnKind::ResultValue { inner } | ReturnKind::RawValue { inner } => {
            let ty_str = quote! { #inner }.to_string().replace(' ', "");
            ty_str == "BinaryResponse" || ty_str.ends_with("::BinaryResponse")
        }
        _ => false,
    }
}

/// Check if any parameter is `BinaryUpload`.
fn has_binary_upload_param(method: &ParsedMethod) -> bool {
    method.params.iter().any(|p| {
        let ty = &p.owned_type;
        let ty_str = quote! { #ty }.to_string().replace(' ', "");
        ty_str == "BinaryUpload" || ty_str.ends_with("::BinaryUpload")
    })
}

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
                ::simply_rpc::MethodMeta {
                    name: #name,
                    signature_hash: #hash,
                }
            }
        })
        .collect();

    // Generate route metadata entries (REST + stream)
    let route_metas: Vec<TokenStream> = parsed
        .methods
        .iter()
        .filter_map(|m| {
            let endpoint = m.rest_endpoint.as_ref()?;
            let kind = route_kind_tokens(&endpoint.http_method);
            let path = &endpoint.path_template;
            let method_name = &m.method_name;
            let description = match &m.doc_comment {
                Some(doc) => quote! { Some(#doc) },
                None => quote! { None },
            };
            let no_tool = m.no_tool;
            let immutable_cache = m.immutable_cache;
            let binary_response = is_binary_response_type(&m.return_kind);
            let binary_upload = has_binary_upload_param(m);
            Some(quote! {
                ::simply_rpc::RouteMeta {
                    kind: #kind,
                    path_template: #path,
                    method_name: #method_name,
                    description: #description,
                    no_tool: #no_tool,
                    binary_response: #binary_response,
                    binary_upload: #binary_upload,
                    immutable_cache: #immutable_cache,
                }
            })
        })
        .collect();

    // Generate rest_dispatch match arms
    let rest_dispatch_arms = generate_rest_dispatch_arms(parsed);
    let content_dispatch_arms = generate_content_dispatch_arms(parsed);

    Ok(quote! {
        /// Auto-generated RPC service wrapper (internal — use `TraitName::service()` instead).
        #[doc(hidden)]
        #vis struct #service_name<T: #trait_name + ?Sized>(::std::sync::Arc<T>);

        /// Auto-generated method metadata for compatibility checking.
        #vis const #meta_const_name: ::simply_rpc::ServiceMeta = ::simply_rpc::ServiceMeta {
            prefix: #prefix,
            methods: &[#(#method_metas),*],
            routes: &[#(#route_metas),*],
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
        impl<T: #trait_name + ?Sized + 'static> ::simply_rpc::RestService for #service_name<T> {
            async fn rest_dispatch_by_name(
                &self,
                method_name: &str,
                params: ::serde_json::Value,
            ) -> Option<::simply_rpc::RpcResult> {
                #rest_dispatch_arms
            }

            async fn rest_dispatch_as_content(
                &self,
                method_name: &str,
                params: ::serde_json::Value,
            ) -> Option<::anyhow::Result<Vec<::simply_rpc::ContentPart>>> {
                #content_dispatch_arms
            }

            fn meta(&self) -> &'static ::simply_rpc::ServiceMeta {
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

        let deser = quote! {
            let #name: #owned_type = match ::serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return Some(::simply_rpc::DispatchResult::value(
                    Err(::anyhow::anyhow!("deserialize error: {}", e))
                )),
            };
        };

        let arg = if p.is_ref || p.is_str_ref {
            quote! { &#name }
        } else {
            quote! { #name }
        };

        return (deser, vec![arg]);
    }

    // Multi-param: generate a Params struct
    let struct_name = format_ident!("__RpcParams_{}", method.name);
    let fields: Vec<TokenStream> = params
        .iter()
        .map(|p| {
            let name = &p.name;
            let owned_type = &p.owned_type;
            quote! { #name: #owned_type }
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
            if p.is_ref || p.is_str_ref {
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

/// Generate the body of `rest_dispatch_by_name` — a match on method name.
///
/// The `RestDispatcher` (using matchit) handles path matching and param extraction.
/// By the time this is called, `params` is a flat JSON object with all parameters
/// (path params as strings, body params as-is).
fn generate_rest_dispatch_arms(parsed: &ParsedTrait) -> TokenStream {
    let arms: Vec<TokenStream> = parsed
        .methods
        .iter()
        .filter_map(|m| {
            let endpoint = m.rest_endpoint.as_ref()?;
            if endpoint.http_method == HttpMethod::Stream {
                return None;
            }

            let method_name = &m.method_name;
            let fn_name = &m.name;

            // All params come from the merged JSON object.
            // For methods with no params or a single non-path param with no path params,
            // the body may be the raw value. Otherwise it's a JSON object.
            let has_path_params = !endpoint.path_params.is_empty();
            let all_params = &m.params;

            let mut param_bindings = Vec::new();
            let mut call_args = Vec::new();

            if all_params.is_empty() {
                // No params — nothing to deserialize
            } else if all_params.len() == 1 && !has_path_params {
                // Single param, no path params — params IS the value directly
                let p = &all_params[0];
                let name = &p.name;
                let owned_type = &p.owned_type;
                param_bindings.push(quote! {
                    let #name: #owned_type = match ::serde_json::from_value(params.clone()) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(::anyhow::anyhow!("deserialize error: {}", e))),
                    };
                });
                if p.is_ref || p.is_str_ref {
                    call_args.push(quote! { &#name });
                } else {
                    call_args.push(quote! { #name });
                }
            } else {
                // Multiple params or has path params — extract each from JSON object
                for p in all_params {
                    let name = &p.name;
                    let name_str = name.to_string();
                    let owned_type = &p.owned_type;

                    if p.is_str_ref {
                        // &str: extract as string from JSON, borrow it
                        param_bindings.push(quote! {
                            let #name: String = match params.get(#name_str) {
                                Some(v) => match ::serde_json::from_value(v.clone()) {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(::anyhow::anyhow!("deserialize '{}': {}", #name_str, e))),
                                },
                                None => return Some(Err(::anyhow::anyhow!("missing field: {}", #name_str))),
                            };
                        });
                    } else {
                        param_bindings.push(quote! {
                            let #name: #owned_type = match params.get(#name_str) {
                                Some(v) => match ::serde_json::from_value(v.clone()) {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(::anyhow::anyhow!("deserialize '{}': {}", #name_str, e))),
                                },
                                None => return Some(Err(::anyhow::anyhow!("missing field: {}", #name_str))),
                            };
                        });
                    }

                    if p.is_ref || p.is_str_ref {
                        call_args.push(quote! { &#name });
                    } else {
                        call_args.push(quote! { #name });
                    }
                }
            }

            let call = quote! { self.0.#fn_name(#(#call_args),*).await };
            let result_wrap = match &m.return_kind {
                ReturnKind::ResultUnit => quote! {
                    match #call {
                        Ok(()) => Some(Ok(::serde_json::Value::Bool(true))),
                        Err(e) => Some(Err(e)),
                    }
                },
                ReturnKind::ResultValue { .. } => {
                    quote! { Some(::simply_rpc::call_val(#call)) }
                },
                ReturnKind::RawValue { .. } => quote! { Some(::simply_rpc::call_raw(#call)) },
                _ => return None,
            };

            Some(quote! {
                #method_name => {
                    #(#param_bindings)*
                    return #result_wrap;
                }
            })
        })
        .collect();

    if arms.is_empty() {
        quote! { None }
    } else {
        quote! {
            match method_name {
                #(#arms)*
                _ => None,
            }
        }
    }
}

/// Generate `rest_dispatch_as_content` match arms.
///
/// Same param deserialization as `rest_dispatch_by_name`, but converts
/// results via `IntoContent::into_content()` on the concrete return type.
fn generate_content_dispatch_arms(parsed: &ParsedTrait) -> TokenStream {
    let arms: Vec<TokenStream> = parsed
        .methods
        .iter()
        .filter_map(|m| {
            let endpoint = m.rest_endpoint.as_ref()?;
            if endpoint.http_method == HttpMethod::Stream {
                return None;
            }

            let method_name = &m.method_name;
            let fn_name = &m.name;
            let has_path_params = !endpoint.path_params.is_empty();
            let all_params = &m.params;

            // Reuse the same param deserialization logic
            let mut param_bindings = Vec::new();
            let mut call_args = Vec::new();

            if all_params.is_empty() {
                // No params
            } else if all_params.len() == 1 && !has_path_params {
                let p = &all_params[0];
                let name = &p.name;
                let owned_type = &p.owned_type;
                param_bindings.push(quote! {
                    let #name: #owned_type = match ::serde_json::from_value(params.clone()) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(::anyhow::anyhow!("deserialize error: {}", e))),
                    };
                });
                if p.is_ref || p.is_str_ref {
                    call_args.push(quote! { &#name });
                } else {
                    call_args.push(quote! { #name });
                }
            } else {
                for p in all_params {
                    let name = &p.name;
                    let name_str = name.to_string();
                    let owned_type = &p.owned_type;
                    if p.is_str_ref {
                        param_bindings.push(quote! {
                            let #name: String = match params.get(#name_str) {
                                Some(v) => match ::serde_json::from_value(v.clone()) {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(::anyhow::anyhow!("deserialize '{}': {}", #name_str, e))),
                                },
                                None => return Some(Err(::anyhow::anyhow!("missing field: {}", #name_str))),
                            };
                        });
                    } else {
                        param_bindings.push(quote! {
                            let #name: #owned_type = match params.get(#name_str) {
                                Some(v) => match ::serde_json::from_value(v.clone()) {
                                    Ok(v) => v,
                                    Err(e) => return Some(Err(::anyhow::anyhow!("deserialize '{}': {}", #name_str, e))),
                                },
                                None => return Some(Err(::anyhow::anyhow!("missing field: {}", #name_str))),
                            };
                        });
                    }
                    if p.is_ref || p.is_str_ref {
                        call_args.push(quote! { &#name });
                    } else {
                        call_args.push(quote! { #name });
                    }
                }
            }

            let call = quote! { self.0.#fn_name(#(#call_args),*).await };

            // Convert result via IntoContent on the concrete type
            let result_wrap = match &m.return_kind {
                ReturnKind::ResultUnit => quote! {
                    match #call {
                        Ok(()) => Some(Ok(vec![])),
                        Err(e) => Some(Err(e)),
                    }
                },
                ReturnKind::ResultValue { .. } => quote! {
                    match #call {
                        Ok(v) => Some(Ok(::simply_rpc::IntoContent::into_content(v))),
                        Err(e) => Some(Err(e)),
                    }
                },
                ReturnKind::RawValue { .. } => quote! {
                    Some(Ok(::simply_rpc::IntoContent::into_content(#call)))
                },
                _ => return None,
            };

            Some(quote! {
                #method_name => {
                    #(#param_bindings)*
                    return #result_wrap;
                }
            })
        })
        .collect();

    if arms.is_empty() {
        quote! { None }
    } else {
        quote! {
            match method_name {
                #(#arms)*
                _ => None,
            }
        }
    }
}

/// Generate the body that calls the trait method and wraps the result.
fn generate_dispatch_body(method: &ParsedMethod, call_args: &[TokenStream]) -> TokenStream {
    let fn_name = &method.name;
    let call = quote! { self.0.#fn_name(#(#call_args),*).await };

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
