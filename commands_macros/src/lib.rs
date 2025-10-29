use proc_macro::TokenStream;
use syn::{
    parse_macro_input, DeriveInput, ImplItem, ItemImpl,
};

mod completable;
mod command;
mod register;

use completable::impl_completable;
use command::{impl_command, impl_completer};
use register::impl_register_commands;

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

/// Marks a method as a command inside #[commandable] impl blocks
///
/// This is a marker attribute that is processed by #[commandable].
/// It should only be used on methods inside impl blocks annotated with #[commandable].
#[proc_macro_attribute]
pub fn command(_args: TokenStream, input: TokenStream) -> TokenStream {
    // This is just a marker attribute for methods inside #[commandable] impl blocks
    // The actual processing is done by the #[commandable] macro
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

    impl_register_commands(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

// Helper trait for pascal case conversion (used by multiple modules)
pub(crate) trait ToPascalCase {
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
