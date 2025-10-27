use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, FnArg, ItemFn, Pat, PatType};

#[proc_macro_attribute]
pub fn delegate_provider_enum(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let enum_name = &input.ident;

    let variants = match &input.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => panic!("delegate_provider_enum only works on enums"),
    };

    // Extract variant names and their inner types
    let variant_info: Vec<_> = variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            let inner_type = match &variant.fields {
                Fields::Unnamed(fields) => {
                    if fields.unnamed.len() != 1 {
                        panic!("Each variant must have exactly one unnamed field");
                    }
                    &fields.unnamed[0].ty
                }
                _ => panic!("Each variant must have exactly one unnamed field"),
            };
            (variant_name, inner_type)
        })
        .collect();

    // Generate the ChatModel enum name by replacing "Provider" with "ChatModel"
    let model_enum_name = syn::Ident::new(
        &enum_name.to_string().replace("Provider", "ChatModel"),
        enum_name.span(),
    );

    // Generate ChatModel enum variants
    let model_variants = variant_info.iter().map(|(variant_name, inner_type)| {
        // Extract the ChatModel type from the provider type
        // Assumes pattern like OllamaProvider -> OllamaChatModel
        let inner_type_str = quote!(#inner_type).to_string();
        let model_type_str = inner_type_str.replace("Provider", "ChatModel");
        let model_type: syn::Type = syn::parse_str(&model_type_str)
            .expect(&format!("Failed to parse model type: {}", model_type_str));

        quote! {
            #variant_name(#model_type)
        }
    });

    // Generate match arms for list_models
    let list_models_arms = variant_info.iter().map(|(variant_name, _)| {
        quote! {
            #enum_name::#variant_name(provider) => provider.list_models().await
        }
    });

    // Generate match arms for create_chat_model
    let create_chat_model_arms = variant_info.iter().map(|(variant_name, _)| {
        quote! {
            #enum_name::#variant_name(provider) => provider
                .create_chat_model(model_name)
                .map(#model_enum_name::#variant_name)
        }
    });

    // Generate match arms for chat
    let chat_arms = variant_info.iter().map(|(variant_name, _)| {
        quote! {
            #model_enum_name::#variant_name(model) => model.chat(request).await
        }
    });

    // Generate match arms for stream_chat
    let stream_chat_arms = variant_info.iter().map(|(variant_name, _)| {
        quote! {
            #model_enum_name::#variant_name(model) => model.stream_chat(request).await
        }
    });

    let vis = &input.vis;
    let attrs = &input.attrs;

    // Re-emit the original enum variants
    let original_variants = variant_info.iter().map(|(variant_name, inner_type)| {
        quote! {
            #variant_name(#inner_type)
        }
    });

    let expanded = quote! {
        #(#attrs)*
        #vis enum #enum_name {
            #(#original_variants),*
        }

        #vis enum #model_enum_name {
            #(#model_variants),*
        }

        #[::async_trait::async_trait]
        impl crate::ModelProvider for #enum_name {
            type ModelType = #model_enum_name;

            async fn list_models(&self) -> ::anyhow::Result<Vec<String>> {
                match self {
                    #(#list_models_arms),*
                }
            }

            fn create_chat_model(&self, model_name: &str) -> Option<#model_enum_name> {
                match self {
                    #(#create_chat_model_arms),*
                }
            }
        }

        #[::async_trait::async_trait]
        impl crate::ChatModel for #model_enum_name {
            async fn chat(&self, request: &crate::ChatRequest) -> ::anyhow::Result<crate::ChatMessage> {
                match self {
                    #(#chat_arms),*
                }
            }

            async fn stream_chat(&self, request: &crate::ChatRequest) -> ::anyhow::Result<crate::ChatStream> {
                match self {
                    #(#stream_chat_arms),*
                }
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
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

    // Extract function parameters (excluding self)
    let mut params = Vec::new();
    let mut param_names = Vec::new();
    let mut param_types = Vec::new();
    let mut has_self = false;

    for arg in &input.sig.inputs {
        match arg {
            FnArg::Receiver(_) => {
                has_self = true;
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

    // Generate the wrapper function name
    let wrapper_name = syn::Ident::new(&format!("{}_wrapper", fn_name), fn_name.span());

    // Generate the call to the original function
    let fn_call = if has_self {
        quote! {
            self.#fn_name(#(args.#param_names),*)
        }
    } else {
        quote! {
            #fn_name(#(args.#param_names),*)
        }
    };

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
        quote! {
            Some({
                let mut desc = String::new();
                #(
                    if let Some(doc) = #doc_attrs.meta.require_name_value().ok() {
                        if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &doc.value {
                            let line = s.value().trim();
                            if !line.is_empty() {
                                if !desc.is_empty() {
                                    desc.push(' ');
                                }
                                desc.push_str(line);
                            }
                        }
                    }
                )*
                desc
            })
        }
    } else {
        quote! { None }
    };

    // Generate method to get tool definition
    let tool_def_method_name = syn::Ident::new(&format!("{}_tool_def", fn_name), fn_name.span());

    let expanded = if has_self {
        // For methods, generate implementations on the type
        quote! {
            #(#fn_attrs)*
            #fn_vis #fn_asyncness fn #fn_name(&self, #(#param_names: #param_types),*) #fn_output #fn_block

            #[derive(::serde::Deserialize, ::serde::Serialize, ::schemars::JsonSchema)]
            pub struct #struct_name {
                #(#struct_fields),*
            }

            impl #struct_name {
                pub fn #tool_def_method_name() -> ::llm::ToolDefinition {
                    use ::schemars::schema_for;
                    let schema = schema_for!(#struct_name);
                    ::llm::ToolDefinition {
                        name: #tool_name.to_string(),
                        description: #tool_description,
                        input_schema: schema,
                    }
                }

                pub #wrapper_async fn #wrapper_name(&self, args_json: ::serde_json::Value) -> ::anyhow::Result<String> {
                    let args: #struct_name = ::serde_json::from_value(args_json)?;
                    let result = #fn_call #wrapper_await;
                    Ok(::serde_json::to_string(&result)?)
                }
            }
        }
    } else {
        // For functions, generate standalone wrapper
        quote! {
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
                    let result = #fn_call #wrapper_await;
                    Ok(::serde_json::to_string(&result)?)
                }
            }
        }
    };

    TokenStream::from(expanded)
}
