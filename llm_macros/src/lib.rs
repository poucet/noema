use proc_macro::TokenStream;

mod delegate_provider;
mod tool;
mod tool_methods;

/// Delegates ModelProvider and ChatModel trait implementations to enum variants.
///
/// This macro generates boilerplate for enums that wrap different provider implementations,
/// automatically delegating trait methods to the appropriate variant.
///
/// # Example
///
/// ```ignore
/// #[delegate_provider_enum]
/// pub enum MyProvider {
///     Ollama(OllamaProvider),
///     OpenAI(OpenAIProvider),
/// }
/// ```
#[proc_macro_attribute]
pub fn delegate_provider_enum(attr: TokenStream, item: TokenStream) -> TokenStream {
    delegate_provider::delegate_provider_enum_impl(attr, item)
}

/// Marks a function or method as a tool that can be called by LLMs.
///
/// For standalone functions, this generates an Args struct, tool definition, and call wrapper.
/// For methods (functions with `&self`), this is a marker that works with `#[tool_methods]`.
///
/// # Example - Standalone Function
///
/// ```ignore
/// #[tool]
/// fn add_numbers(a: i32, b: i32) -> i32 {
///     a + b
/// }
///
/// // Generates:
/// // - AddNumbersArgs struct
/// // - AddNumbersArgs::tool_def() method
/// // - AddNumbersArgs::call() wrapper
/// ```
///
/// # Example - Method (with `#[tool_methods]`)
///
/// ```ignore
/// #[tool_methods]
/// impl Calculator {
///     #[tool]
///     fn add(&self, amount: i32) -> i32 {
///         self.base + amount
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    tool::tool_impl(attr, item)
}

/// Processes an impl block to convert `#[tool]`-marked methods into callable tools.
///
/// This macro scans an impl block for methods marked with `#[tool]`, generates
/// Args structs for each at module level, and creates tool definitions and wrappers.
///
/// # Example
///
/// ```ignore
/// use llm_macros::{tool, tool_methods};
///
/// struct Calculator {
///     base: i32,
/// }
///
/// #[tool_methods]
/// impl Calculator {
///     fn new(base: i32) -> Self {
///         Self { base }
///     }
///
///     #[tool]
///     fn add(&self, amount: i32) -> i32 {
///         self.base + amount
///     }
///
///     #[tool]
///     async fn async_compute(&self, x: i32) -> i32 {
///         self.base + x
///     }
/// }
///
/// // Generates for each #[tool] method:
/// // - MethodNameArgs struct
/// // - MethodNameArgs::method_name_tool_def()
/// // - MethodNameArgs::method_name_wrapper(&self, instance: &Calculator, args_json)
/// ```
#[proc_macro_attribute]
pub fn tool_methods(attr: TokenStream, item: TokenStream) -> TokenStream {
    tool_methods::tool_methods_impl(attr, item)
}
