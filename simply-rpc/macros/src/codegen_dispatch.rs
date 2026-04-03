use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::parse::{HttpMethod, ParsedMethod, ParsedTrait, ReturnKind, RpcKind};

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
            let rest_get = m.rest_get;
            let base64_return = m.base64_return;
            quote! {
                ::simply_rpc::MethodMeta {
                    name: #name,
                    signature_hash: #hash,
                    rest_get: #rest_get,
                    base64_return: #base64_return,
                }
            }
        })
        .collect();

    // Generate REST metadata entries
    let rest_metas: Vec<TokenStream> = parsed
        .methods
        .iter()
        .filter_map(|m| {
            let endpoint = m.rest_endpoint.as_ref()?;
            let http_method = match endpoint.http_method {
                HttpMethod::Get => quote! { ::simply_rpc::HttpMethod::Get },
                HttpMethod::Post => quote! { ::simply_rpc::HttpMethod::Post },
                HttpMethod::Put => quote! { ::simply_rpc::HttpMethod::Put },
                HttpMethod::Delete => quote! { ::simply_rpc::HttpMethod::Delete },
            };
            let path = &endpoint.path_template;
            let method_name = &m.method_name;
            let description = match &m.doc_comment {
                Some(doc) => quote! { Some(#doc) },
                None => quote! { None },
            };
            let no_tool = m.no_tool;
            Some(quote! {
                ::simply_rpc::RestMeta {
                    http_method: #http_method,
                    path_template: #path,
                    method_name: #method_name,
                    description: #description,
                    no_tool: #no_tool,
                }
            })
        })
        .collect();

    // Generate rest_dispatch match arms
    let rest_dispatch_arms = generate_rest_dispatch_arms(parsed);

    Ok(quote! {
        /// Auto-generated RPC service wrapper (internal — use `TraitName::service()` instead).
        #[doc(hidden)]
        #vis struct #service_name<T: #trait_name + ?Sized>(::std::sync::Arc<T>);

        /// Auto-generated method metadata for compatibility checking.
        #vis const #meta_const_name: ::simply_rpc::ServiceMeta = ::simply_rpc::ServiceMeta {
            prefix: #prefix,
            methods: &[#(#method_metas),*],
            rest_methods: &[#(#rest_metas),*],
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

        impl<T: #trait_name + ?Sized + 'static> #service_name<T> {
            /// Dispatch an HTTP REST request to a trait method.
            ///
            /// Returns `Some(json_value)` if the path matched, `None` otherwise.
            pub async fn rest_dispatch(
                &self,
                http_method: ::simply_rpc::HttpMethod,
                path: &str,
                body: ::serde_json::Value,
            ) -> Option<::simply_rpc::RpcResult> {
                #rest_dispatch_arms
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

/// Generate the body of `rest_dispatch` — a series of if-let match arms for each REST endpoint.
///
/// For each REST-annotated method, generates pattern matching on the HTTP method and path,
/// extracts path params by position, deserializes remaining params from body/query,
/// calls the trait method, and wraps the result.
fn generate_rest_dispatch_arms(parsed: &ParsedTrait) -> TokenStream {
    let arms: Vec<TokenStream> = parsed
        .methods
        .iter()
        .filter_map(|m| {
            let endpoint = m.rest_endpoint.as_ref()?;
            let http_method_check = match endpoint.http_method {
                HttpMethod::Get => quote! { ::simply_rpc::HttpMethod::Get },
                HttpMethod::Post => quote! { ::simply_rpc::HttpMethod::Post },
                HttpMethod::Put => quote! { ::simply_rpc::HttpMethod::Put },
                HttpMethod::Delete => quote! { ::simply_rpc::HttpMethod::Delete },
            };

            // Build a path pattern: split template into segments, each is either
            // a literal string match or a `{param}` capture.
            let template = &endpoint.path_template;
            let segments: Vec<&str> = template.trim_start_matches('/').split('/').collect();
            let segment_count = segments.len();

            // Generate segment matching code
            let mut segment_checks = Vec::new();
            let mut param_extractions = Vec::new();

            for (i, seg) in segments.iter().enumerate() {
                if seg.starts_with('{') && seg.ends_with('}') {
                    let param_name = &seg[1..seg.len() - 1];
                    let param_ident = format_ident!("__path_{}", param_name);
                    param_extractions.push((param_name.to_string(), param_ident.clone()));
                    segment_checks.push(quote! {
                        let #param_ident = __segments[#i];
                    });
                } else {
                    segment_checks.push(quote! {
                        if __segments[#i] != #seg { return None; }
                    });
                }
            }

            // Build the trait method call
            let fn_name = &m.name;
            let mut call_args = Vec::new();

            // Params not in path come from body (JSON object fields)
            let body_params: Vec<_> = m.params.iter().filter(|p| {
                !endpoint.path_params.contains(&p.name.to_string())
            }).collect();

            // Deserialize body params if needed
            let body_deser = if body_params.is_empty() {
                quote! {}
            } else if body_params.len() == 1 && endpoint.path_params.is_empty() {
                // Single non-path param, whole body is the value
                let p = &body_params[0];
                let name = &p.name;
                let owned_type = &p.owned_type;
                quote! {
                    let #name: #owned_type = match ::serde_json::from_value(body) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(::anyhow::anyhow!("deserialize error: {}", e))),
                    };
                }
            } else {
                // Multiple body params — extract from JSON object
                let field_extractions: Vec<TokenStream> = body_params.iter().map(|p| {
                    let name = &p.name;
                    let name_str = name.to_string();
                    let owned_type = &p.owned_type;
                    quote! {
                        let #name: #owned_type = match body.get(#name_str) {
                            Some(v) => match ::serde_json::from_value(v.clone()) {
                                Ok(v) => v,
                                Err(e) => return Some(Err(::anyhow::anyhow!("deserialize '{}': {}", #name_str, e))),
                            },
                            None => return Some(Err(::anyhow::anyhow!("missing field: {}", #name_str))),
                        };
                    }
                }).collect();
                quote! { #(#field_extractions)* }
            };

            // Build call args in the original parameter order
            for p in &m.params {
                let name = &p.name;
                let name_str = name.to_string();
                if let Some((_, ident)) = param_extractions.iter().find(|(n, _)| n == &name_str) {
                    // Path param — extracted as &str, need to deserialize to owned type
                    let owned_type = &p.owned_type;
                    if p.is_str_ref {
                        call_args.push(quote! { #ident });
                    } else if p.is_ref {
                        let local = format_ident!("__local_{}", name);
                        call_args.push(quote! { {
                            let #local: #owned_type = match ::serde_json::from_value(
                                ::serde_json::Value::String(#ident.to_string())
                            ) {
                                Ok(v) => v,
                                Err(e) => return Some(Err(::anyhow::anyhow!("path param '{}': {}", #name_str, e))),
                            };
                            &#local
                        } });
                    } else {
                        call_args.push(quote! { {
                            match ::serde_json::from_value::<#owned_type>(
                                ::serde_json::Value::String(#ident.to_string())
                            ) {
                                Ok(v) => v,
                                Err(e) => return Some(Err(::anyhow::anyhow!("path param '{}': {}", #name_str, e))),
                            }
                        } });
                    }
                } else if p.is_ref || p.is_str_ref {
                    call_args.push(quote! { &#name });
                } else {
                    call_args.push(quote! { #name });
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
                    if m.base64_return {
                        quote! {
                            match #call {
                                Ok(bytes) => Some(Ok(::serde_json::Value::String(
                                    ::simply_rpc::encode_base64(&bytes)
                                ))),
                                Err(e) => Some(Err(e)),
                            }
                        }
                    } else {
                        quote! {
                            Some(::simply_rpc::call_val(#call))
                        }
                    }
                },
                ReturnKind::RawValue { .. } => quote! {
                    Some(::simply_rpc::call_raw(#call))
                },
                _ => return None, // Stream methods don't get REST dispatch
            };

            Some(quote! {
                if http_method == #http_method_check {
                    let __segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
                    if __segments.len() == #segment_count {
                        #(#segment_checks)*
                        #body_deser
                        return #result_wrap;
                    }
                }
            })
        })
        .collect();

    if arms.is_empty() {
        quote! { None }
    } else {
        quote! {
            #(#arms)*
            None
        }
    }
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
