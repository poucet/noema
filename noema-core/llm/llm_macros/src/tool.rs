use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, Pat, PatType};

pub fn tool_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    // Check if this is a method (has &self parameter)
    // If so, pass it through as a no-op (without the #[tool] attribute)
    // The #[tool_methods] macro will handle methods by looking for the original #[tool] attribute
    let has_self = input.sig.inputs.iter().any(|arg| matches!(arg, FnArg::Receiver(_)));

    if has_self {
        // This is a method - just pass through unchanged WITHOUT re-emitting #[tool]
        // This makes rust-analyzer happy and #[tool_methods] will see the original attribute
        return quote! {
            #input
        }
        .into();
    }

    let fn_name = &input.sig.ident;
    let fn_vis = &input.vis;
    let fn_block = &input.block;
    let fn_attrs = &input.attrs;
    let fn_asyncness = &input.sig.asyncness;
    let fn_output = &input.sig.output;

    // Extract function documentation from attributes
    let doc_attrs: Vec<_> = fn_attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .collect();

    // Generate struct name: function_name -> FunctionNameArgs
    let struct_name = syn::Ident::new(
        &format!(
            "{}Args",
            fn_name
                .to_string()
                .split('_')
                .map(|s| {
                    let mut c = s.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect::<String>()
        ),
        fn_name.span(),
    );

    // Extract function parameters
    let mut params = Vec::new();
    let mut param_names = Vec::new();
    let mut param_types = Vec::new();

    for arg in &input.sig.inputs {
        match arg {
            FnArg::Receiver(_) => {
                // Already checked above
            }
            FnArg::Typed(PatType { pat, ty, attrs, .. }) => {
                if let Pat::Ident(pat_ident) = pat.as_ref() {
                    let param_name = &pat_ident.ident;
                    param_names.push(param_name.clone());
                    param_types.push(ty.clone());
                    params.push((param_name.clone(), ty.clone(), attrs.clone()));
                }
            }
        }
    }

    // Generate the struct fields with their attributes
    let struct_fields = params.iter().map(|(name, ty, attrs)| {
        quote! {
            #(#attrs)*
            pub #name: #ty
        }
    });

    // Determine if wrapper should be async
    let wrapper_async = if fn_asyncness.is_some() {
        quote! { async }
    } else {
        quote! {}
    };

    let wrapper_await = if fn_asyncness.is_some() {
        quote! { .await }
    } else {
        quote! {}
    };

    // Generate the tool name from the function name
    let tool_name = fn_name.to_string();

    // Generate tool description from doc comments
    let tool_description = if !doc_attrs.is_empty() {
        let doc_strings: Vec<_> = doc_attrs
            .iter()
            .filter_map(|attr| {
                if let Ok(name_value) = attr.meta.require_name_value() {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &name_value.value
                    {
                        let value = s.value();
                        let line = value.trim();
                        if !line.is_empty() {
                            return Some(line.to_string());
                        }
                    }
                }
                None
            })
            .collect();

        if doc_strings.is_empty() {
            quote! { None }
        } else {
            let combined = doc_strings.join(" ");
            quote! { Some(#combined.to_string()) }
        }
    } else {
        quote! { None }
    };

    // For standalone functions, generate Args struct and wrapper
    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_asyncness fn #fn_name(#(#param_names: #param_types),*) #fn_output #fn_block

        #[derive(::serde::Deserialize, ::serde::Serialize, ::schemars::JsonSchema)]
        pub struct #struct_name {
            #(#struct_fields),*
        }

        impl #struct_name {
            pub fn tool_def() -> ::llm::ToolDefinition {
                use ::schemars::schema_for;
                let schema = schema_for!(#struct_name);
                ::llm::ToolDefinition {
                    name: #tool_name.to_string(),
                    description: #tool_description,
                    input_schema: schema,
                }
            }

            pub #wrapper_async fn call(args_json: ::serde_json::Value) -> ::anyhow::Result<String> {
                let args: #struct_name = ::serde_json::from_value(args_json)?;
                let result = #fn_name(#(args.#param_names),*) #wrapper_await;
                Ok(::serde_json::to_string(&result)?)
            }
        }
    };

    TokenStream::from(expanded)
}
