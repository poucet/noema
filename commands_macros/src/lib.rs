use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

/// Makes an enum automatically completable with case-insensitive matching
///
/// Generates:
/// - `FromStr` impl with lowercase matching
/// - `AsyncCompleter` impl returning all variants
///
/// # Example
/// ```ignore
/// #[completable]
/// enum Provider {
///     #[completion(description = "Local LLM")]
///     Ollama,
///     Gemini,
/// }
/// ```
#[proc_macro_attribute]
pub fn completable(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let enum_name = &input.ident;
    let variants = match &input.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => {
            return syn::Error::new_spanned(
                enum_name,
                "#[completable] can only be used on enums",
            )
            .to_compile_error()
            .into();
        }
    };

    // Extract variant information
    let mut variant_names = Vec::new();
    let mut variant_values = Vec::new();
    let mut variant_labels = Vec::new();
    let mut variant_descriptions = Vec::new();

    for variant in variants {
        if !matches!(variant.fields, Fields::Unit) {
            return syn::Error::new_spanned(
                variant,
                "#[completable] only supports unit variants",
            )
            .to_compile_error()
            .into();
        }

        let variant_name = &variant.ident;
        variant_names.push(variant_name);

        // Convert to lowercase for value
        let value = variant_name.to_string().to_lowercase();
        variant_values.push(value.clone());

        // Default label is the variant name as-is
        let label = variant_name.to_string();
        variant_labels.push(label);

        // Extract description from attributes if present
        let mut description = None;
        for attr in &variant.attrs {
            if attr.path().is_ident("completion") {
                if let Ok(meta_list) = attr.parse_args::<syn::MetaList>() {
                    // Parse description = "..."
                    if let Ok(name_value) = meta_list.parse_args::<syn::MetaNameValue>() {
                        if name_value.path.is_ident("description") {
                            if let syn::Expr::Lit(expr_lit) = &name_value.value {
                                if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                                    description = Some(lit_str.value());
                                }
                            }
                        }
                    }
                }
            }
        }
        variant_descriptions.push(description);
    }

    // Generate FromStr impl
    let from_str_arms = variant_names.iter().zip(&variant_values).map(|(name, value)| {
        quote! {
            #value => Ok(#enum_name::#name),
        }
    });

    // Generate completion entries
    let completion_entries = variant_names
        .iter()
        .zip(&variant_values)
        .zip(&variant_labels)
        .zip(&variant_descriptions)
        .map(|(((_name, value), label), desc)| {
            let desc_opt = if let Some(d) = desc {
                quote! { Some(#d.to_string()) }
            } else {
                quote! { None }
            };

            quote! {
                ::commands::Completion {
                    value: #value.to_string(),
                    label: Some(#label.to_string()),
                    description: #desc_opt,
                    metadata: None,
                }
            }
        });

    let expanded = quote! {
        #input

        impl ::std::str::FromStr for #enum_name {
            type Err = String;

            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                match s.to_lowercase().as_str() {
                    #(#from_str_arms)*
                    _ => ::std::result::Result::Err(format!("Unknown {}: {}", stringify!(#enum_name), s)),
                }
            }
        }

        #[::commands::async_trait::async_trait]
        impl ::commands::AsyncCompleter for #enum_name {
            type Metadata = ();

            async fn complete(
                &self,
                partial: &str,
                _context: &::commands::CompletionContext,
            ) -> ::std::result::Result<Vec<::commands::Completion<()>>, ::commands::CompletionError> {
                let partial_lower = partial.to_lowercase();
                let variants = vec![
                    #(#completion_entries),*
                ];

                Ok(variants
                    .into_iter()
                    .filter(|c| c.value.starts_with(&partial_lower))
                    .collect())
            }
        }
    };

    TokenStream::from(expanded)
}

/// Marks a method as a command (placeholder - will implement later)
#[proc_macro_attribute]
pub fn command(_args: TokenStream, input: TokenStream) -> TokenStream {
    // TODO: Implement command macro
    input
}

/// Marks a method as a completer for a specific argument (placeholder)
#[proc_macro_attribute]
pub fn completer(_args: TokenStream, input: TokenStream) -> TokenStream {
    // TODO: Implement completer macro
    input
}
