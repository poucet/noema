use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::parse::{HttpMethod, ParsedMethod, ParsedTrait, RestEndpoint, ReturnKind, RpcKind};

/// Generate the `impl_remote_xxx!` declarative macro.
pub fn generate(parsed: &ParsedTrait) -> syn::Result<TokenStream> {
    let macro_name = parsed.client_macro_name();
    let trait_name = &parsed.trait_name;

    let method_impls: Vec<TokenStream> = parsed
        .methods
        .iter()
        .map(|m| generate_client_method(m))
        .collect::<syn::Result<Vec<_>>>()?;

    // Collect stream types for where bounds on the client
    let stream_methods: Vec<_> = parsed
        .methods
        .iter()
        .filter(|m| m.rpc_kind == RpcKind::Stream)
        .collect();

    // No where bounds needed — the RpcClient::Stream type must match
    // the stream types in the trait. The compiler enforces this via the
    // explicit type annotations in the generated method bodies.
    let _ = stream_methods;

    Ok(quote! {
        /// Implement the trait for any type implementing `RpcClient`.
        macro_rules! #macro_name {
            ($T:ty) => {
                #[::async_trait::async_trait]
                impl #trait_name for $T
                {
                    #(#method_impls)*
                }
            };
        }
    })
}

/// Generate a single method implementation for the client.
fn generate_client_method(method: &ParsedMethod) -> syn::Result<TokenStream> {
    let sig = &method.sig;
    let fn_name = &sig.ident;
    let inputs = &sig.inputs;
    let output = &sig.output;

    if method.rpc_kind == RpcKind::Skip {
        return Ok(quote! {
            async fn #fn_name(#inputs) #output {
                ::anyhow::bail!(concat!(stringify!(#fn_name), " is not available over RPC"))
            }
        });
    }

    // REST-annotated methods (not stream) → generate rest_call with path interpolation
    if let Some(endpoint) = &method.rest_endpoint {
        if endpoint.http_method != HttpMethod::Stream {
            let body = generate_rest_client_body(method, endpoint);
            return Ok(quote! {
                async fn #fn_name(#inputs) #output {
                    use ::simply_rpc::RpcClient;
                    #body
                }
            });
        }
    }

    // Stream or unannotated methods → WS rpc_call (existing behavior)
    let method_str = &method.method_name;
    let (serialize, rpc_params) = generate_serialize(method);
    let body = generate_client_body(method, method_str, &serialize, &rpc_params);

    Ok(quote! {
        async fn #fn_name(#inputs) #output {
            use ::simply_rpc::RpcClient;
            #body
        }
    })
}

/// Generate the body for a REST-annotated client method.
///
/// Builds the path from the template (interpolating params), collects remaining
/// params into a JSON body, then calls `self.rest_call(method, path, body)`.
fn generate_rest_client_body(method: &ParsedMethod, endpoint: &RestEndpoint) -> TokenStream {
    let http_method = match endpoint.http_method {
        HttpMethod::Get => quote! { ::simply_rpc::HttpMethod::Get },
        HttpMethod::Post => quote! { ::simply_rpc::HttpMethod::Post },
        HttpMethod::Put => quote! { ::simply_rpc::HttpMethod::Put },
        HttpMethod::Delete => quote! { ::simply_rpc::HttpMethod::Delete },
        HttpMethod::Stream => unreachable!("stream methods handled separately"),
    };

    // Build path string with interpolation
    let path_template = &endpoint.path_template;
    let path_expr = if endpoint.path_params.is_empty() {
        quote! { #path_template.to_string() }
    } else {
        // Replace {param} with the actual param value
        let mut format_str = path_template.clone();
        let mut format_args = Vec::new();
        for param_name in &endpoint.path_params {
            let placeholder = format!("{{{param_name}}}");
            format_str = format_str.replace(&placeholder, "{}");
            let param_ident = format_ident!("{}", param_name);
            format_args.push(quote! { #param_ident });
        }
        quote! { format!(#format_str, #(#format_args),*) }
    };

    // Body params: everything not in the path
    let body_params: Vec<_> = method.params.iter().filter(|p| {
        !endpoint.path_params.contains(&p.name.to_string())
    }).collect();

    let body_expr = if body_params.is_empty() {
        quote! { ::serde_json::Value::Null }
    } else if body_params.len() == 1 && endpoint.path_params.is_empty() {
        // Single param, no path params → whole body is the value
        let p = &body_params[0];
        let name = &p.name;
        quote! { ::serde_json::to_value(#name)? }
    } else {
        let struct_name = format_ident!("__RestClientParams_{}", method.name);
        let fields: Vec<TokenStream> = body_params.iter().map(|p| {
            let name = &p.name;
            let owned_type = &p.owned_type;
            quote! { #name: #owned_type }
        }).collect();
        let field_inits: Vec<TokenStream> = body_params.iter().map(|p| {
            let name = &p.name;
            if p.is_str_ref {
                quote! { #name: #name.to_string() }
            } else if p.is_ref {
                quote! { #name: #name.clone() }
            } else {
                quote! { #name }
            }
        }).collect();
        quote! {
            {
                #[derive(::serde::Serialize)]
                struct #struct_name { #(#fields,)* }
                ::serde_json::to_value(&#struct_name { #(#field_inits,)* })?
            }
        }
    };

    match &method.return_kind {
        ReturnKind::ResultUnit => quote! {
            let __path = #path_expr;
            self.rest_call(#http_method, &__path, #body_expr).await?;
            Ok(())
        },
        ReturnKind::ResultValue { .. } => quote! {
            let __path = #path_expr;
            let __r = self.rest_call(#http_method, &__path, #body_expr).await?;
            Ok(::serde_json::from_value(__r)?)
        },
        ReturnKind::RawValue { .. } => quote! {
            let __path = #path_expr;
            self.rest_call(#http_method, &__path, #body_expr)
                .await
                .and_then(|r| ::serde_json::from_value(r).map_err(Into::into))
                .unwrap_or_default()
        },
        _ => unreachable!("stream return types handled separately"),
    }
}

/// Generate serialization code for the client call.
fn generate_serialize(method: &ParsedMethod) -> (TokenStream, TokenStream) {
    let params = &method.params;

    if params.is_empty() {
        return (quote! {}, quote! { ::serde_json::Value::Null });
    }

    if params.len() == 1 {
        let p = &params[0];
        let name = &p.name;
        return (quote! {}, quote! { ::serde_json::to_value(#name)? });
    }

    // Multi-param: generate a Params struct with owned types for serialization.
    let struct_name = format_ident!("__RpcClientParams_{}", method.name);
    let fields: Vec<TokenStream> = params
        .iter()
        .map(|p| {
            let name = &p.name;
            let owned_type = &p.owned_type;
            quote! { #name: #owned_type }
        })
        .collect();

    let field_inits: Vec<TokenStream> = params
        .iter()
        .map(|p| {
            let name = &p.name;
            if p.is_str_ref {
                quote! { #name: #name.to_string() }
            } else if p.is_ref {
                quote! { #name: #name.clone() }
            } else {
                quote! { #name }
            }
        })
        .collect();

    let serialize = quote! {
        #[derive(::serde::Serialize)]
        struct #struct_name {
            #(#fields,)*
        }
    };

    let params_expr = quote! {
        ::serde_json::to_value(&#struct_name {
            #(#field_inits,)*
        })?
    };

    (serialize, params_expr)
}

/// Check if a type is `Vec<u8>`.
fn is_vec_u8(ty: &syn::Type) -> bool {
    let s = quote! { #ty }.to_string().replace(' ', "");
    s == "Vec<u8>"
}

/// Generate the body of a client method.
fn generate_client_body(
    method: &ParsedMethod,
    method_str: &str,
    serialize: &TokenStream,
    rpc_params: &TokenStream,
) -> TokenStream {
    match &method.return_kind {
        ReturnKind::ResultUnit => {
            quote! {
                #serialize
                self.rpc_call(#method_str, #rpc_params).await?;
                Ok(())
            }
        }
        ReturnKind::ResultValue { .. } => {
            quote! {
                #serialize
                let __r = self.rpc_call(#method_str, #rpc_params).await?;
                Ok(::serde_json::from_value(__r)?)
            }
        }
        ReturnKind::RawValue { .. } => {
            // RawValue methods don't return Result, so we can't use ? — wrap everything in a closure
            let params_for_raw = if method.params.is_empty() {
                quote! { ::serde_json::Value::Null }
            } else if method.params.len() == 1 {
                let name = &method.params[0].name;
                quote! { ::serde_json::to_value(#name).unwrap_or_default() }
            } else {
                // Multi-param: reuse the serialize struct but with unwrap
                quote! { #rpc_params.unwrap_or_default() }
            };
            quote! {
                #serialize
                self.rpc_call(#method_str, #params_for_raw)
                    .await
                    .and_then(|r| ::serde_json::from_value(r).map_err(Into::into))
                    .unwrap_or_default()
            }
        }
        ReturnKind::StreamTuple { value_type, stream_type } => {
            // Result<(T, S)> — RPC returns T, then register stream
            quote! {
                #serialize
                let __r = self.rpc_call(#method_str, #rpc_params).await?;
                let __value: #value_type = ::serde_json::from_value(__r)?;
                let __stream: #stream_type = self.register_stream(__value.id.as_str()).await;
                Ok((__value, __stream))
            }
        }
        ReturnKind::StreamBare { stream_type } => {
            // Result<S> — RPC returns true, register stream using first param
            let id_expr = if let Some(first) = method.params.first() {
                let name = &first.name;
                if first.is_str_ref {
                    quote! { #name }
                } else {
                    quote! { #name.as_str() }
                }
            } else {
                quote! { "" }
            };

            quote! {
                #serialize
                self.rpc_call(#method_str, #rpc_params).await?;
                let __stream: #stream_type = self.register_stream(#id_expr).await;
                Ok(__stream)
            }
        }
        ReturnKind::StreamBidi { input_type, output_type } => {
            // Result<StreamHandle<T, U>> — open a bidi stream via RPC
            // The RPC call sets up the server side. Then we register for
            // incoming events (U) and get a way to send messages (T).
            quote! {
                #serialize
                self.rpc_call(#method_str, #rpc_params).await?;
                let __handle: ::simply_rpc::StreamHandle<#input_type, #output_type> =
                    self.register_bidi_stream(#method_str).await?;
                Ok(__handle)
            }
        }
    }
}

/// Collect unique stream types from stream methods.
fn collect_unique_stream_types(methods: &[&ParsedMethod]) -> Vec<syn::Type> {
    let mut types = Vec::new();
    for m in methods {
        let st = match &m.return_kind {
            ReturnKind::StreamTuple { stream_type, .. } => stream_type,
            ReturnKind::StreamBare { stream_type } => stream_type,
            ReturnKind::StreamBidi { .. } => continue, // bidi uses register_bidi_stream, not Stream type
            _ => continue,
        };
        let s = quote::quote! { #st }.to_string();
        if !types.iter().any(|t| quote::quote! { #t }.to_string() == s) {
            types.push(st.clone());
        }
    }
    types
}
