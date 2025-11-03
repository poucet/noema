use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

pub fn delegate_provider_enum_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
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

    // Generate match arms for list_models
    let list_models_arms = variant_info.iter().map(|(variant_name, _)| {
        quote! {
            #enum_name::#variant_name(provider) => provider.list_models().await
        }
    });

    // Generate match arms for create_chat_model
    // Providers now return Arc<dyn ChatModel>, so just pass through
    let create_chat_model_arms = variant_info.iter().map(|(variant_name, _)| {
        quote! {
            #enum_name::#variant_name(provider) => provider.create_chat_model(model_name)
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

        #[::async_trait::async_trait]
        impl crate::ModelProvider for #enum_name {
            async fn list_models(&self) -> ::anyhow::Result<Vec<crate::ModelDefinition>> {
                match self {
                    #(#list_models_arms),*
                }
            }

            fn create_chat_model(&self, model_name: &str) -> Option<::std::sync::Arc<dyn crate::ChatModel + Send + Sync>> {
                match self {
                    #(#create_chat_model_arms),*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
