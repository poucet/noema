use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Result};

pub fn impl_completable(input: DeriveInput) -> Result<TokenStream> {
    let enum_name = &input.ident;
    let variants = match &input.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => {
            return Err(syn::Error::new_spanned(
                enum_name,
                "#[completable] can only be used on enums",
            ));
        }
    };

    // Extract variant information
    let mut variant_names = Vec::new();
    let mut variant_values = Vec::new();
    let mut variant_labels = Vec::new();
    let mut variant_descriptions = Vec::new();

    for variant in variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                variant,
                "#[completable] only supports unit variants",
            ));
        }

        let variant_name = &variant.ident;
        variant_names.push(variant_name);

        // Convert to lowercase for value
        let value = variant_name.to_string().to_lowercase();
        variant_values.push(value.clone());

        // Default label is the variant name as-is
        let label = variant_name.to_string();
        variant_labels.push(label);

        // Extract description from doc comments
        let mut description = None;
        for attr in &variant.attrs {
            if attr.path().is_ident("doc") {
                if let syn::Meta::NameValue(meta) = &attr.meta {
                    if let syn::Expr::Lit(expr_lit) = &meta.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            let doc_str = lit_str.value().trim().to_string();
                            if !doc_str.is_empty() {
                                description = Some(doc_str);
                                break; // Use first doc comment
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
    let completion_entries = variant_values
        .iter()
        .zip(&variant_labels)
        .zip(&variant_descriptions)
        .map(|((value, label), desc)| {
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

    // Get first variant for Default impl
    let first_variant = &variant_names[0];

    Ok(quote! {
        #input

        impl ::std::default::Default for #enum_name {
            fn default() -> Self {
                #enum_name::#first_variant
            }
        }

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
        impl ::commands::AsyncCompleter<()> for #enum_name {
            async fn complete(
                &self,
                context: &::commands::CompletionContext<()>,
            ) -> ::std::result::Result<Vec<::commands::Completion>, ::commands::CompletionError> {
                let partial_lower = context.partial().to_lowercase();
                let variants = vec![
                    #(#completion_entries),*
                ];

                Ok(variants
                    .into_iter()
                    .filter(|c| c.value.starts_with(&partial_lower))
                    .collect())
            }
        }
    })
}
