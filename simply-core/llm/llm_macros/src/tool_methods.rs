use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, ImplItem, ItemImpl, Pat, PatType};

pub fn tool_methods_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Early return on parse error to help rust-analyzer
    let input = match syn::parse::<ItemImpl>(item.clone()) {
        Ok(input) => input,
        Err(_) => return item, // Pass through on error
    };

    let self_ty = &input.self_ty;

    let mut methods_to_process = Vec::new();
    let mut other_items = Vec::new();

    // Separate methods from other items
    // Look for methods marked with #[tool] attribute
    for item in input.items {
        if let ImplItem::Fn(method) = item {
            // Check if this method has the #[tool] attribute
            let has_tool_attr = method.attrs.iter().any(|attr| attr.path().is_ident("tool"));

            if has_tool_attr {
                methods_to_process.push(method);
            } else {
                other_items.push(ImplItem::Fn(method));
            }
        } else {
            other_items.push(item);
        }
    }

    // Generate Args structs and their impls
    let mut generated_structs = Vec::new();
    let mut cleaned_methods = Vec::new();

    for mut method in methods_to_process {
        // Remove the #[tool] attribute from the method
        method.attrs.retain(|attr| !attr.path().is_ident("tool"));

        let fn_name = &method.sig.ident;
        let fn_attrs = &method.attrs;
        let fn_asyncness = &method.sig.asyncness;
        let fn_output = &method.sig.output;
        let fn_block = &method.block;

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

        for arg in &method.sig.inputs {
            match arg {
                FnArg::Receiver(_) => {
                    // Skip self parameter
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
        let fn_call = quote! {
            instance.#fn_name(#(args.#param_names),*)
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

        // Generate method to get tool definition
        let tool_def_method_name =
            syn::Ident::new(&format!("{}_tool_def", fn_name), fn_name.span());

        // Generate the Args struct and its impl
        let struct_def = quote! {
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

                pub #wrapper_async fn #wrapper_name(&self, instance: &#self_ty, args_json: ::serde_json::Value) -> ::anyhow::Result<String> {
                    let args: #struct_name = ::serde_json::from_value(args_json)?;
                    let result = #fn_call #wrapper_await;
                    Ok(::serde_json::to_string(&result)?)
                }
            }
        };

        generated_structs.push(struct_def);

        // Create the cleaned method (without #[tool] attribute)
        let cleaned_method = quote! {
            #(#fn_attrs)*
            #fn_asyncness fn #fn_name(&self, #(#param_names: #param_types),*) #fn_output #fn_block
        };

        cleaned_methods.push(cleaned_method);
    }

    // Reconstruct the impl block
    let impl_generics = &input.generics;
    let where_clause = &input.generics.where_clause;

    let expanded = quote! {
        // Generate all Args structs at module level
        #(#generated_structs)*

        // Generate the impl block with cleaned methods
        impl #impl_generics #self_ty #where_clause {
            #(#cleaned_methods)*
            #(#other_items)*
        }
    };

    TokenStream::from(expanded)
}
