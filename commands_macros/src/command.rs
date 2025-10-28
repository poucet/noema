use proc_macro2::TokenStream;
use quote::{quote, format_ident, ToTokens};
use syn::{
    FnArg, ImplItem, ItemImpl, Pat, PatType, Result, ReturnType,
    Type, Ident, spanned::Spanned,
};

/// Information about a command method
struct CommandInfo {
    method_name: Ident,
    self_type: Type,
    args: Vec<ArgInfo>,
    return_type: ReturnType,
    command_name: String,
    help_text: String,
}

/// Information about a method argument
struct ArgInfo {
    name: Ident,
    ty: Type,
    is_optional: bool,
}

/// Parse command attributes to extract name and help text
fn parse_command_attrs(impl_block: &ItemImpl) -> Result<Vec<(String, String)>> {
    let mut commands = Vec::new();

    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item {
            let mut command_name = None;
            let mut help_text = String::new();

            for attr in &method.attrs {
                if attr.path().is_ident("command") {
                    // Parse #[command(name = "...", help = "...")]
                    attr.parse_nested_meta(|meta| {
                        if meta.path.is_ident("name") {
                            let value = meta.value()?;
                            let s: syn::LitStr = value.parse()?;
                            command_name = Some(s.value());
                        } else if meta.path.is_ident("help") {
                            let value = meta.value()?;
                            let s: syn::LitStr = value.parse()?;
                            help_text = s.value();
                        }
                        Ok(())
                    })?;
                }
            }

            if let Some(name) = command_name {
                commands.push((name, help_text));
            }
        }
    }

    Ok(commands)
}

/// Parse argument information from PatType
fn parse_arg_info(pat_type: &PatType) -> Result<ArgInfo> {
    let name = if let Pat::Ident(pat_ident) = &*pat_type.pat {
        pat_ident.ident.clone()
    } else {
        return Err(syn::Error::new(
            pat_type.pat.span(),
            "Expected simple identifier pattern",
        ));
    };

    let ty = (*pat_type.ty).clone();

    // Check if type is Option<T>
    let is_optional = is_option_type(&ty);

    Ok(ArgInfo {
        name,
        ty,
        is_optional,
    })
}

/// Check if a type is Option<T>
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

/// Generate the command wrapper struct and implementations
fn generate_command_wrapper(info: &CommandInfo) -> TokenStream {
    let self_type = &info.self_type;
    let method_name = &info.method_name;
    let command_name = &info.command_name;
    let help_text = &info.help_text;

    // Generate wrapper struct name (e.g., AppSetModelCommand)
    let wrapper_name = format_ident!(
        "{}{}Command",
        self_type.to_token_stream().to_string().replace(" ", ""),
        method_name.to_string().to_pascal_case()
    );

    // Generate argument parsing code
    let parse_args = info.args.iter().enumerate().map(|(i, arg)| {
        let arg_name = &arg.name;
        let arg_ty = &arg.ty;

        if arg.is_optional {
            quote! {
                let #arg_name = args.parse_optional::<#arg_ty>(#i)
                    .map_err(|e| ::commands::CommandError::ParseError(e))?;
            }
        } else {
            quote! {
                let #arg_name = args.parse::<#arg_ty>(#i)
                    .map_err(|e| ::commands::CommandError::ParseError(e))?;
            }
        }
    });

    // Generate argument names for method call
    let arg_names: Vec<_> = info.args.iter().map(|arg| &arg.name).collect();

    // Generate result conversion based on return type
    let result_conversion = generate_result_conversion(&info.return_type);

    // Generate completion routing for each argument
    let completion_arms = info.args.iter().enumerate().map(|(i, arg)| {
        let arg_ty = &arg.ty;

        // Try to use the type's AsyncCompleter impl if it exists
        // This will work for types annotated with #[completable]
        quote! {
            #i => {
                // Try to complete using the type's AsyncCompleter implementation
                // This requires the type to implement AsyncCompleter (e.g., via #[completable])
                let dummy_value = <#arg_ty as ::std::default::Default>::default();
                dummy_value.complete(partial, context).await
            }
        }
    });

    let num_args = info.args.len();

    quote! {
        pub struct #wrapper_name {
            inner: ::std::sync::Arc<::tokio::sync::Mutex<#self_type>>,
        }

        impl #wrapper_name {
            pub fn new(inner: ::std::sync::Arc<::tokio::sync::Mutex<#self_type>>) -> Self {
                Self { inner }
            }
        }

        #[::commands::async_trait::async_trait]
        impl ::commands::AsyncCompleter for #wrapper_name {
            type Metadata = ();

            async fn complete(
                &self,
                partial: &str,
                context: &::commands::CompletionContext,
            ) -> ::std::result::Result<Vec<::commands::Completion<()>>, ::commands::CompletionError> {
                // Determine which argument we're completing
                // tokens includes command name, so arg_index = tokens.len() - 1
                let arg_index = if context.tokens.len() > 1 {
                    context.tokens.len() - 2 // -1 for command, -1 for 0-indexing
                } else {
                    0
                };

                // Route to appropriate completer based on argument position
                match arg_index {
                    #(#completion_arms)*
                    _ if arg_index >= #num_args => {
                        // No more arguments to complete
                        Ok(vec![])
                    }
                    _ => Ok(vec![])
                }
            }
        }

        #[::commands::async_trait::async_trait]
        impl ::commands::Command for #wrapper_name {
            async fn execute(
                &mut self,
                args: ::commands::ParsedArgs,
            ) -> ::std::result::Result<::commands::CommandResult, ::commands::CommandError> {
                let mut inner = self.inner.lock().await;

                // Parse arguments
                #(#parse_args)*

                // Call user method
                let result = inner.#method_name(#(#arg_names),*).await;

                // Convert result to CommandResult
                #result_conversion
            }

            fn metadata(&self) -> &::commands::CommandMetadata {
                static METADATA: ::commands::CommandMetadata = ::commands::CommandMetadata {
                    name: #command_name,
                    help: #help_text,
                };
                &METADATA
            }
        }

        /// Helper function to create the command wrapper
        pub fn #method_name(
            inner: ::std::sync::Arc<::tokio::sync::Mutex<#self_type>>
        ) -> #wrapper_name {
            #wrapper_name::new(inner)
        }
    }
}

/// Generate result conversion code based on return type
fn generate_result_conversion(return_type: &ReturnType) -> TokenStream {
    match return_type {
        ReturnType::Default => {
            // No return type - infallible
            quote! {
                Ok(::commands::CommandResult::Success(String::new()))
            }
        }
        ReturnType::Type(_, ty) => {
            // Check if it's Result<T, E>
            if is_result_type(ty) {
                // Check if the Ok type is () by looking at the type
                let is_unit_result = is_result_unit_type(ty);

                if is_unit_result {
                    quote! {
                        match result {
                            Ok(()) => Ok(::commands::CommandResult::Success(String::new())),
                            Err(e) => Err(::commands::CommandError::ExecutionError(e.to_string())),
                        }
                    }
                } else {
                    quote! {
                        match result {
                            Ok(msg) => {
                                let message = format!("{}", msg);
                                Ok(::commands::CommandResult::Success(message))
                            }
                            Err(e) => Err(::commands::CommandError::ExecutionError(e.to_string())),
                        }
                    }
                }
            } else {
                // Direct return type
                quote! {
                    let message = format!("{}", result);
                    Ok(::commands::CommandResult::Success(message))
                }
            }
        }
    }
}

/// Check if a type is Result<(), E>
fn is_result_unit_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                // Check if the first type argument is ()
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Tuple(tuple))) = args.args.first() {
                        return tuple.elems.is_empty(); // () is an empty tuple
                    }
                }
            }
        }
    }
    false
}

/// Check if a type is Result<T, E>
fn is_result_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Result";
        }
    }
    false
}

/// Convert snake_case to PascalCase
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

impl ToPascalCase for str {
    fn to_pascal_case(&self) -> String {
        self.to_string().to_pascal_case()
    }
}

/// Extract command information for a specific command by name
fn extract_command_info_by_name(
    impl_block: &ItemImpl,
    command_name: &str,
    help_text: &str,
) -> Result<CommandInfo> {
    let self_type = if let Type::Path(type_path) = &*impl_block.self_ty {
        Type::Path(type_path.clone())
    } else {
        return Err(syn::Error::new(
            impl_block.self_ty.span(),
            "Expected a simple type path",
        ));
    };

    // Find the specific command method by name attribute
    let method = impl_block
        .items
        .iter()
        .find_map(|item| {
            if let ImplItem::Fn(method) = item {
                for attr in &method.attrs {
                    if attr.path().is_ident("command") {
                        // Check if this command has the matching name
                        let mut matches = false;
                        let _ = attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("name") {
                                if let Ok(value) = meta.value() {
                                    if let Ok(s) = value.parse::<syn::LitStr>() {
                                        if s.value() == command_name {
                                            matches = true;
                                        }
                                    }
                                }
                            }
                            Ok(())
                        });
                        if matches {
                            return Some(method);
                        }
                    }
                }
            }
            None
        })
        .ok_or_else(|| {
            syn::Error::new(impl_block.span(), format!("No method with #[command(name = \"{}\")] found", command_name))
        })?;

    let method_name = method.sig.ident.clone();
    let return_type = method.sig.output.clone();

    // Parse arguments (skip self)
    let mut args = Vec::new();
    for arg in &method.sig.inputs {
        match arg {
            FnArg::Receiver(_) => continue, // Skip self
            FnArg::Typed(pat_type) => {
                let arg_info = parse_arg_info(pat_type)?;
                args.push(arg_info);
            }
        }
    }

    Ok(CommandInfo {
        method_name,
        self_type,
        args,
        return_type,
        command_name: command_name.to_string(),
        help_text: help_text.to_string(),
    })
}

/// Strip #[command] attributes from impl block
fn strip_command_attrs(mut input: ItemImpl) -> ItemImpl {
    for item in &mut input.items {
        if let ImplItem::Fn(method) = item {
            method.attrs.retain(|attr| !attr.path().is_ident("command"));
        }
    }
    input
}

/// Implement the #[command] macro
pub fn impl_command(input: ItemImpl) -> Result<TokenStream> {
    // Parse command attributes
    let commands = parse_command_attrs(&input)?;

    if commands.is_empty() {
        // No commands found, return original impl block
        return Ok(quote! { #input });
    }

    // Strip #[command] attributes from the impl block for output
    let cleaned_input = strip_command_attrs(input.clone());

    // Generate wrappers for all commands
    let mut wrappers = Vec::new();
    for (command_name, help_text) in &commands {
        let info = extract_command_info_by_name(&input, command_name, help_text)?;
        let wrapper = generate_command_wrapper(&info);
        wrappers.push(wrapper);
    }

    Ok(quote! {
        #cleaned_input

        #(#wrappers)*
    })
}

/// Implement the #[completer] macro
pub fn impl_completer(input: ImplItem) -> Result<TokenStream> {
    // TODO: Store completer metadata for use by #[command] macro
    // For now, just return the original method
    Ok(quote! {
        #input
    })
}
