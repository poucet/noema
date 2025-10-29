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

            async fn list_models(&self) -> ::anyhow::Result<Vec<crate::ModelDefinition>> {
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
