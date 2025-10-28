use proc_macro::TokenStream;
use quote::format_ident;
use syn::{
    parse_macro_input, DeriveInput, ImplItem, ItemImpl,
};

mod completable;
mod command;

use completable::impl_completable;
use command::{impl_command, impl_command_function, impl_completer};

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
    impl_completable(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Marks an impl block as containing commands
///
/// # Example
/// ```ignore
/// #[commandable]
/// impl App {
///     #[command(name = "model", help = "Switch model provider")]
///     async fn set_model(&mut self, provider: Provider) -> Result<String, anyhow::Error> {
///         // implementation
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn commandable(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemImpl);

    impl_command(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Marks a single method or function as a command (for standalone commands)
///
/// Can be used on:
/// - Methods inside #[commandable] impl blocks (marker attribute)
/// - Standalone free functions (generates Command implementation)
#[proc_macro_attribute]
pub fn command(_args: TokenStream, input: TokenStream) -> TokenStream {
    // Try to parse as ItemFn (free function)
    if let Ok(func) = syn::parse::<syn::ItemFn>(input.clone()) {
        match command::impl_command_function(func) {
            Ok(tokens) => return tokens.into(),
            Err(e) => return e.to_compile_error().into(),
        }
    }

    // Otherwise, it's a marker attribute for methods inside #[commandable]
    input
}

/// Marks a method as a completer for a specific argument
///
/// # Example
/// ```ignore
/// impl App {
///     #[completer(arg = "model")]
///     async fn complete_model_name(
///         &self,
///         provider: &Provider,
///         partial: &str
///     ) -> Result<Vec<Completion>, anyhow::Error> {
///         // implementation
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn completer(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as ImplItem);

    impl_completer(item)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Helper macro to register commands without knowing struct names
///
/// # Example
/// ```ignore
/// register_commands!(registry, App => [cmd_help, cmd_clear, cmd_model]);
/// ```
#[proc_macro]
pub fn register_commands(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);

    // Parse: registry, Type => [method1, method2, ...]
    let parsed: syn::Result<(syn::Expr, syn::Type, Vec<syn::Ident>)> = (|| {
        use syn::parse::Parser;
        use syn::punctuated::Punctuated;
        use syn::Token;

        let parser = |input: syn::parse::ParseStream| {
            let registry: syn::Expr = input.parse()?;
            input.parse::<Token![,]>()?;
            let type_name: syn::Type = input.parse()?;
            input.parse::<Token![=>]>()?;

            let content;
            syn::bracketed!(content in input);
            let methods = Punctuated::<syn::Ident, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();

            Ok((registry, type_name, methods))
        };

        parser.parse2(input)
    })();

    match parsed {
        Ok((registry, type_name, methods)) => {
            let registrations = methods.iter().map(|method| {
                // Generate struct name from method name (e.g., cmd_help → AppCmdHelpCommand)
                let struct_name = format_ident!(
                    "{}{}Command",
                    quote::quote!(#type_name).to_string().replace(" ", ""),
                    method.to_string().to_pascal_case()
                );

                quote::quote! {
                    #registry.register_cmd::<#struct_name>();
                }
            });

            quote::quote! {
                #(#registrations)*
            }.into()
        }
        Err(e) => e.to_compile_error().into()
    }
}

// Helper trait for pascal case conversion
trait ToPascalCase {
    fn to_pascal_case(&self) -> String;
}

impl ToPascalCase for String {
    fn to_pascal_case(&self) -> String {
        self.split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect()
    }
}
