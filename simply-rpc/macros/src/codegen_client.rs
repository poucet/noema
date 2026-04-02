use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::parse::{ParsedMethod, ParsedTrait, ReturnKind, RpcKind};

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
        #[macro_export]
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
    // We use owned types to avoid lifetime issues in the generated struct.
    let struct_name = format_ident!("__RpcClientParams_{}", method.name);
    let fields: Vec<TokenStream> = params
        .iter()
        .map(|p| {
            let name = &p.name;
            let owned_type = &p.owned_type;
            quote! { #name: #owned_type }
        })
        .collect();

    // Convert refs to owned for struct init
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
            quote! {
                #serialize
                self.rpc_call(#method_str, #rpc_params)
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
    }
}

/// Collect unique stream types from stream methods.
fn collect_unique_stream_types(methods: &[&ParsedMethod]) -> Vec<syn::Type> {
    let mut types = Vec::new();
    for m in methods {
        let st = match &m.return_kind {
            ReturnKind::StreamTuple { stream_type, .. } => stream_type,
            ReturnKind::StreamBare { stream_type } => stream_type,
            _ => continue,
        };
        let s = quote::quote! { #st }.to_string();
        if !types.iter().any(|t| quote::quote! { #t }.to_string() == s) {
            types.push(st.clone());
        }
    }
    types
}
