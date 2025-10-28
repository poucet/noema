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
    completers: std::collections::HashMap<String, Ident>, // arg_name -> completer_method_name
}

/// Information about a method argument
struct ArgInfo {
    name: Ident,
    ty: Type,
    is_optional: bool,
    /// Inner type for Option<T> (for completion routing)
    inner_ty: Option<Type>,
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

    // Check if type is Option<T> and extract inner type
    let (is_optional, inner_ty) = extract_option_inner_type(&ty);

    Ok(ArgInfo {
        name,
        ty,
        is_optional,
        inner_ty,
    })
}

/// Check if a type is Option<T> and extract T
fn extract_option_inner_type(ty: &Type) -> (bool, Option<Type>) {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                // Extract the T from Option<T>
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return (true, Some(inner.clone()));
                    }
                }
                return (true, None);
            }
        }
    }
    (false, None)
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

        if arg.is_optional {
            // For Option<T>, use the inner type
            if let Some(ref inner_ty) = arg.inner_ty {
                quote! {
                    let #arg_name = args.parse_optional::<#inner_ty>(#i)
                        .map_err(|e| ::commands::CommandError::ParseError(e))?;
                }
            } else {
                // Fallback if we couldn't extract inner type
                let arg_ty = &arg.ty;
                quote! {
                    let #arg_name: #arg_ty = args.parse_optional(#i)
                        .map_err(|e| ::commands::CommandError::ParseError(e))?;
                }
            }
        } else {
            let arg_ty = &arg.ty;
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

    // Generate completion routing for enum-based arguments (no target needed)
    let enum_completion_arms = info.args.iter().enumerate().filter_map(|(i, arg)| {
        let arg_name_str = arg.name.to_string();

        // Skip arguments with custom completers (they need target access)
        if info.completers.contains_key(&arg_name_str) {
            return None;
        }

        // Use inner type for Option<T>, otherwise use the type itself
        let completion_ty = if arg.is_optional {
            arg.inner_ty.as_ref()?
        } else {
            &arg.ty
        };

        // Skip non-completable built-in types
        if is_builtin_type(completion_ty) {
            return None;
        }

        // Generate completion for completable types (enums with #[completable])
        Some(quote! {
            #i => {
                let dummy_value = <#completion_ty as ::std::default::Default>::default();
                dummy_value.complete(partial, context).await
            }
        })
    });

    let num_args = info.args.len();
    let has_custom_completers = !info.completers.is_empty();

    // Generate complete_with_target override if there are custom completers
    let complete_with_target_impl = if has_custom_completers {
        // Find completion arms that use custom completers (need target access)
        let completion_with_target_arms = info.args.iter().enumerate().filter_map(|(i, arg)| {
            let arg_name_str = arg.name.to_string();

            if let Some(completer_method) = info.completers.get(&arg_name_str) {
                let prev_args_parsing: Vec<_> = info.args.iter().take(i).enumerate().map(|(j, prev_arg)| {
                    let prev_name = &prev_arg.name;
                    let prev_ty = if prev_arg.is_optional {
                        prev_arg.inner_ty.as_ref().unwrap_or(&prev_arg.ty)
                    } else {
                        &prev_arg.ty
                    };

                    quote! {
                        let #prev_name = context.tokens.get(#j + 1)
                            .and_then(|s| s.parse::<#prev_ty>().ok());
                    }
                }).collect();

                let prev_arg_names: Vec<_> = info.args.iter().take(i).map(|a| &a.name).collect();
                let prev_arg_refs: Vec<_> = prev_arg_names.iter().map(|name| {
                    quote! { &#name }
                }).collect();

                Some(quote! {
                    #i => {
                        #(#prev_args_parsing)*

                        if vec![#(#prev_arg_names.is_some()),*].iter().all(|&x| x) {
                            target.#completer_method(#(#prev_arg_refs.unwrap()),*, partial).await
                                .map_err(|e| ::commands::CompletionError::Custom(e.to_string()))
                        } else {
                            Ok(vec![])
                        }
                    }
                })
            } else {
                None
            }
        });

        quote! {
            async fn complete_with_target(
                &self,
                target: &#self_type,
                partial: &str,
                context: &::commands::CompletionContext,
            ) -> ::std::result::Result<Vec<::commands::Completion<()>>, ::commands::CompletionError> {
                let arg_index = if context.tokens.len() > 1 {
                    context.tokens.len() - 2
                } else {
                    0
                };

                match arg_index {
                    #(#completion_with_target_arms)*
                    _ => self.complete(partial, context).await
                }
            }
        }
    } else {
        quote! {}
    };

    quote! {
        // Zero-sized command struct (no state!)
        pub struct #wrapper_name;

        #[::commands::async_trait::async_trait]
        impl ::commands::AsyncCompleter for #wrapper_name {
            type Metadata = ();

            async fn complete(
                &self,
                partial: &str,
                context: &::commands::CompletionContext,
            ) -> ::std::result::Result<Vec<::commands::Completion<()>>, ::commands::CompletionError> {
                // Base completion without target (only for enums)
                let arg_index = if context.tokens.len() > 1 {
                    context.tokens.len() - 2
                } else {
                    0
                };

                // Only use enum-based completions here (no target access)
                match arg_index {
                    #(#enum_completion_arms)*
                    _ => Ok(vec![])
                }
            }
        }

        #[::commands::async_trait::async_trait]
        impl ::commands::Command<#self_type> for #wrapper_name {
            async fn execute(
                &self,
                target: &mut #self_type,
                args: ::commands::ParsedArgs,
            ) -> ::std::result::Result<::commands::CommandResult, ::commands::CommandError> {
                // Parse arguments
                #(#parse_args)*

                // Call user method
                let result = target.#method_name(#(#arg_names),*).await;

                // Convert result to CommandResult
                #result_conversion
            }

            #complete_with_target_impl

            fn metadata(&self) -> &::commands::CommandMetadata {
                static METADATA: ::commands::CommandMetadata = ::commands::CommandMetadata {
                    name: #command_name,
                    help: #help_text,
                };
                &METADATA
            }
        }

        /// Helper function to create the command (zero-sized, just returns the type)
        pub fn #method_name() -> #wrapper_name {
            #wrapper_name
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

/// Check if a type is a built-in type that doesn't support completion
fn is_builtin_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let ident_str = segment.ident.to_string();
            matches!(
                ident_str.as_str(),
                "String" | "str" | "i8" | "i16" | "i32" | "i64" | "i128" |
                "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "isize" |
                "f32" | "f64" | "bool" | "char"
            )
        } else {
            false
        }
    } else {
        false
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

    // Find completer methods for this command
    let completers = find_completers_for_command(impl_block, command_name)?;

    Ok(CommandInfo {
        method_name,
        self_type,
        args,
        return_type,
        command_name: command_name.to_string(),
        help_text: help_text.to_string(),
        completers,
    })
}

/// Find completer methods in the impl block
fn find_completers_for_command(
    impl_block: &ItemImpl,
    _command_name: &str,
) -> Result<std::collections::HashMap<String, Ident>> {
    let mut completers = std::collections::HashMap::new();

    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item {
            for attr in &method.attrs {
                if attr.path().is_ident("completer") {
                    // Parse #[completer(arg = "arg_name")]
                    let mut arg_name = None;
                    attr.parse_nested_meta(|meta| {
                        if meta.path.is_ident("arg") {
                            let value = meta.value()?;
                            let s: syn::LitStr = value.parse()?;
                            arg_name = Some(s.value());
                        }
                        Ok(())
                    })?;

                    if let Some(arg) = arg_name {
                        completers.insert(arg, method.sig.ident.clone());
                    }
                }
            }
        }
    }

    Ok(completers)
}

/// Strip #[command] and #[completer] attributes from impl block
fn strip_command_attrs(mut input: ItemImpl) -> ItemImpl {
    for item in &mut input.items {
        if let ImplItem::Fn(method) = item {
            method.attrs.retain(|attr| {
                !attr.path().is_ident("command") && !attr.path().is_ident("completer")
            });
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
