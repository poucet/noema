use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

struct ProviderVariantInfo {
    variant_name: syn::Ident,
    inner_type: syn::Type,
    name: String,
    api_key_env: Option<String>,
    base_url_env: String,
}

fn parse_provider_attr(attrs: &[syn::Attribute]) -> Option<(String, Option<String>, String)> {
    for attr in attrs {
        if attr.path().is_ident("provider") {
            let mut name = None;
            let mut api_key_env = None;
            let mut base_url_env = None;

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    name = Some(value.value());
                } else if meta.path.is_ident("api_key_env") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    api_key_env = Some(value.value());
                } else if meta.path.is_ident("base_url_env") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    base_url_env = Some(value.value());
                }
                Ok(())
            }).ok()?;

            if let (Some(n), Some(b)) = (name, base_url_env) {
                return Some((n, api_key_env, b));
            }
        }
    }
    None
}

pub fn delegate_provider_enum_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let enum_name = &input.ident;

    let variants = match &input.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => panic!("delegate_provider_enum only works on enums"),
    };

    // Extract variant info including #[provider(...)] attributes
    let variant_info: Vec<ProviderVariantInfo> = variants
        .iter()
        .map(|variant| {
            let variant_name = variant.ident.clone();
            let inner_type = match &variant.fields {
                Fields::Unnamed(fields) => {
                    if fields.unnamed.len() != 1 {
                        panic!("Each variant must have exactly one unnamed field");
                    }
                    fields.unnamed[0].ty.clone()
                }
                _ => panic!("Each variant must have exactly one unnamed field"),
            };

            let (name, api_key_env, base_url_env) = parse_provider_attr(&variant.attrs)
                .unwrap_or_else(|| panic!(
                    "Variant {} must have #[provider(name = \"...\", base_url_env = \"...\")] attribute",
                    variant_name
                ));

            ProviderVariantInfo {
                variant_name,
                inner_type,
                name,
                api_key_env,
                base_url_env,
            }
        })
        .collect();

    // Generate match arms for list_models
    let list_models_arms = variant_info.iter().map(|info| {
        let variant_name = &info.variant_name;
        quote! {
            #enum_name::#variant_name(provider) => provider.list_models().await
        }
    });

    // Generate match arms for create_chat_model
    let create_chat_model_arms = variant_info.iter().map(|info| {
        let variant_name = &info.variant_name;
        quote! {
            #enum_name::#variant_name(provider) => provider.create_chat_model(model_name)
        }
    });

    // Generate match arms for provider_name
    let provider_name_arms = variant_info.iter().map(|info| {
        let variant_name = &info.variant_name;
        let name = &info.name;
        quote! {
            #enum_name::#variant_name(_) => #name
        }
    });

    // Generate match arms for api_key_env
    let api_key_env_arms = variant_info.iter().map(|info| {
        let variant_name = &info.variant_name;
        let api_key_env = match &info.api_key_env {
            Some(env) => quote! { Some(#env) },
            None => quote! { None },
        };
        quote! {
            #enum_name::#variant_name(_) => #api_key_env
        }
    });

    // Generate match arms for base_url_env
    let base_url_env_arms = variant_info.iter().map(|info| {
        let variant_name = &info.variant_name;
        let base_url_env = &info.base_url_env;
        quote! {
            #enum_name::#variant_name(_) => #base_url_env
        }
    });

    // Generate available_providers array
    let provider_names: Vec<_> = variant_info.iter().map(|info| &info.name).collect();

    // Generate from_name match arms (env var only)
    let from_name_arms = variant_info.iter().map(|info| {
        let variant_name = &info.variant_name;
        let name = &info.name;
        let inner_type = &info.inner_type;
        let api_key_env = &info.api_key_env;
        let base_url_env = &info.base_url_env;

        let create_provider = if api_key_env.is_some() {
            let api_key_env_str = api_key_env.as_ref().unwrap();
            quote! {
                let api_key = ::std::env::var(#api_key_env_str)
                    .map_err(|_| ::anyhow::anyhow!("{} environment variable not set", #api_key_env_str))?;
                let provider = match ::std::env::var(#base_url_env).ok() {
                    Some(url) => <#inner_type>::new(&url, &api_key),
                    None => <#inner_type>::default(&api_key),
                };
                Ok(#enum_name::#variant_name(provider))
            }
        } else {
            quote! {
                let provider = match ::std::env::var(#base_url_env).ok() {
                    Some(url) => <#inner_type>::new(&url),
                    None => <#inner_type>::default(),
                };
                Ok(#enum_name::#variant_name(provider))
            }
        };

        quote! {
            #name => { #create_provider }
        }
    });

    // Generate from_name_with_key match arms (settings key takes priority, falls back to env var)
    let from_name_with_key_arms = variant_info.iter().map(|info| {
        let variant_name = &info.variant_name;
        let name = &info.name;
        let inner_type = &info.inner_type;
        let api_key_env = &info.api_key_env;
        let base_url_env = &info.base_url_env;

        let create_provider = if api_key_env.is_some() {
            let api_key_env_str = api_key_env.as_ref().unwrap();
            quote! {
                // Settings API key takes priority, then fall back to env var
                let api_key = match api_key {
                    Some(key) => key.to_string(),
                    None => ::std::env::var(#api_key_env_str)
                        .map_err(|_| ::anyhow::anyhow!("{} not configured in settings and {} environment variable not set", #name, #api_key_env_str))?,
                };
                let provider = match ::std::env::var(#base_url_env).ok() {
                    Some(url) => <#inner_type>::new(&url, &api_key),
                    None => <#inner_type>::default(&api_key),
                };
                Ok(#enum_name::#variant_name(provider))
            }
        } else {
            // Providers without API keys (like Ollama)
            quote! {
                let provider = match ::std::env::var(#base_url_env).ok() {
                    Some(url) => <#inner_type>::new(&url),
                    None => <#inner_type>::default(),
                };
                Ok(#enum_name::#variant_name(provider))
            }
        };

        quote! {
            #name => { #create_provider }
        }
    });

    // Generate provider_info entries
    let provider_info_entries = variant_info.iter().map(|info| {
        let name = &info.name;
        let api_key_env = match &info.api_key_env {
            Some(env) => quote! { Some(#env) },
            None => quote! { None },
        };
        let base_url_env = &info.base_url_env;
        quote! {
            crate::registry::ProviderInfo {
                name: #name,
                api_key_env: #api_key_env,
                base_url_env: #base_url_env,
            }
        }
    });

    let vis = &input.vis;
    let attrs: Vec<_> = input.attrs.iter().collect();

    // Re-emit the original enum variants (without the #[provider] attributes)
    let original_variants = variant_info.iter().map(|info| {
        let variant_name = &info.variant_name;
        let inner_type = &info.inner_type;
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

        impl #enum_name {
            /// Get the provider name for this variant
            pub fn provider_name(&self) -> &'static str {
                match self {
                    #(#provider_name_arms),*
                }
            }

            /// Get the API key environment variable for this variant
            pub fn api_key_env(&self) -> Option<&'static str> {
                match self {
                    #(#api_key_env_arms),*
                }
            }

            /// Get the base URL environment variable for this variant
            pub fn base_url_env(&self) -> &'static str {
                match self {
                    #(#base_url_env_arms),*
                }
            }

            /// List all available provider names
            pub fn available_providers() -> &'static [&'static str] {
                &[#(#provider_names),*]
            }

            /// Create a provider from its name, reading configuration from environment
            pub fn from_name(name: &str) -> ::anyhow::Result<Self> {
                match name {
                    #(#from_name_arms),*
                    _ => Err(::anyhow::anyhow!("Unknown provider: {}", name))
                }
            }

            /// Create a provider from its name with an optional API key.
            /// If api_key is Some, it takes priority. Otherwise falls back to env var.
            pub fn from_name_with_key(name: &str, api_key: Option<&str>) -> ::anyhow::Result<Self> {
                match name {
                    #(#from_name_with_key_arms),*
                    _ => Err(::anyhow::anyhow!("Unknown provider: {}", name))
                }
            }

            /// Get provider info for all providers
            pub fn all_provider_info() -> &'static [crate::registry::ProviderInfo] {
                static PROVIDERS: &[crate::registry::ProviderInfo] = &[
                    #(#provider_info_entries),*
                ];
                PROVIDERS
            }
        }
    };

    TokenStream::from(expanded)
}
