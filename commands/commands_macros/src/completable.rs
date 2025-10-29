use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Result, Ident};

/// Information about an enum variant for completion
struct VariantInfo {
    name: Ident,
    value: String,      // lowercase version for matching
    label: String,      // original name for display
    description: Option<String>,
}

/// Extract description from doc comment attributes
fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter()
        .find_map(|attr| {
            if attr.path().is_ident("doc") {
                if let syn::Meta::NameValue(meta) = &attr.meta {
                    if let syn::Expr::Lit(expr_lit) = &meta.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            let doc_str = lit_str.value().trim().to_string();
                            return if doc_str.is_empty() { None } else { Some(doc_str) };
                        }
                    }
                }
            }
            None
        })
}

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

    // Process all variants in a single pass
    let variant_infos: Result<Vec<_>> = variants.iter()
        .map(|variant| {
            if !matches!(variant.fields, Fields::Unit) {
                return Err(syn::Error::new_spanned(
                    variant,
                    "#[completable] only supports unit variants",
                ));
            }

            let name = variant.ident.clone();
            let name_str = name.to_string();

            Ok(VariantInfo {
                name,
                value: name_str.to_lowercase(),
                label: name_str,
                description: extract_doc_comment(&variant.attrs),
            })
        })
        .collect();
    let variant_infos = variant_infos?;

    // Generate FromStr match arms
    let from_str_arms = variant_infos.iter().map(|info| {
        let name = &info.name;
        let value = &info.value;
        quote! {
            #value => Ok(#enum_name::#name),
        }
    });

    // Generate completion entries for Completable::completions()
    let completion_entries = variant_infos.iter().map(|info| {
        let value = &info.value;
        let label = &info.label;
        let desc_opt = info.description.as_ref()
            .map(|d| quote! { Some(#d.to_string()) })
            .unwrap_or_else(|| quote! { None });

        quote! {
            ::commands::Completion {
                value: #value.to_string(),
                label: Some(#label.to_string()),
                description: #desc_opt,
                metadata: None,
            }
        }
    });

    Ok(quote! {
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

        impl ::commands::Completable for #enum_name {
            fn completions() -> Vec<::commands::Completion> {
                vec![
                    #(#completion_entries),*
                ]
            }
        }
    })
}
