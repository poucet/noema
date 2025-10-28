use proc_macro::TokenStream;
use syn::{
    parse_macro_input, DeriveInput, ImplItem, ItemImpl,
};

mod completable;
mod command;

use completable::impl_completable;
use command::{impl_command, impl_completer};

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

/// Marks a method as a command
///
/// # Example
/// ```ignore
/// impl App {
///     #[command(name = "model", help = "Switch model provider")]
///     async fn set_model(&mut self, provider: Provider) -> Result<String, anyhow::Error> {
///         // implementation
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn command(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemImpl);

    impl_command(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
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
