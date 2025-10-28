use proc_macro2::TokenStream;
use quote::{quote, format_ident, ToTokens};
use syn::{
    FnArg, ImplItem, ItemImpl, ItemFn, Pat, PatType, Result, ReturnType,
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

/// Get the type to use for parsing/completion (unwraps Option<T> to T)
fn get_effective_type(arg: &ArgInfo) -> &Type {
    if arg.is_optional {
        arg.inner_ty.as_ref().unwrap_or(&arg.ty)
    } else {
        &arg.ty
    }
}

/// Generate a single argument parsing statement
fn generate_arg_parse(arg: &ArgInfo, index: usize) -> TokenStream {
    let arg_name = &arg.name;

    if arg.is_optional {
        if let Some(ref inner_ty) = arg.inner_ty {
            quote! {
                let #arg_name = args.parse_optional::<#inner_ty>(#index)
                    .map_err(|e| ::commands::CommandError::ParseError(e))?;
            }
        } else {
            let arg_ty = &arg.ty;
            quote! {
                let #arg_name: #arg_ty = args.parse_optional(#index)
                    .map_err(|e| ::commands::CommandError::ParseError(e))?;
            }
        }
    } else {
        let arg_ty = &arg.ty;
        quote! {
            let #arg_name = args.parse::<#arg_ty>(#index)
                .map_err(|e| ::commands::CommandError::ParseError(e))?;
        }
    }
}

/// Generate arg_index calculation (uses context method)
fn generate_arg_index_calc() -> TokenStream {
    quote! {
        let arg_index = context.arg_index();
    }
}

/// Generate completion arm for enum-based completion
fn generate_enum_completion_arm(
    arg_index: usize,
    arg: &ArgInfo,
    completers: &std::collections::HashMap<String, Ident>,
) -> Option<TokenStream> {
    let arg_name_str = arg.name.to_string();

    // Skip arguments with custom completers
    if completers.contains_key(&arg_name_str) {
        return None;
    }

    let completion_ty = get_effective_type(arg);

    // Skip non-completable built-in types
    if is_builtin_type(completion_ty) {
        return None;
    }

    Some(quote! {
        #arg_index => {
            let dummy_value = <#completion_ty as ::std::default::Default>::default();
            // Enum completions use () as target - create unit context
            let unit_ctx = ::commands::CompletionContext::new(
                context.input.clone(),
                context.cursor,
                &()
            );
            dummy_value.complete(partial, &unit_ctx).await
        }
    })
}

/// Generate completion arm for custom completer
fn generate_custom_completion_arm(
    arg_index: usize,
    _arg: &ArgInfo,
    all_args: &[ArgInfo],
    completer_method: &Ident,
) -> TokenStream {
    // Parse all previous arguments using TokenStream.parse()
    let prev_args_parsing: Vec<_> = all_args.iter().take(arg_index).enumerate().map(|(j, prev_arg)| {
        let prev_name = &prev_arg.name;
        let prev_ty = get_effective_type(prev_arg);
        let token_index = j + 1; // +1 to skip command name

        quote! {
            let #prev_name = context.tokens.parse::<#prev_ty>(#token_index);
        }
    }).collect();

    let prev_arg_names: Vec<_> = all_args.iter().take(arg_index).map(|a| &a.name).collect();
    let prev_arg_refs: Vec<_> = prev_arg_names.iter().map(|name| {
        quote! { &#name }
    }).collect();

    quote! {
        #arg_index => {
            #(#prev_args_parsing)*

            if vec![#(#prev_arg_names.is_some()),*].iter().all(|&x| x) {
                target.#completer_method(#(#prev_arg_refs.unwrap()),*, partial).await
                    .map_err(|e| ::commands::CompletionError::Custom(e.to_string()))
            } else {
                Ok(vec![])
            }
        }
    }
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

    // Generate argument parsing code using DRY helper
    let parse_args = info.args.iter().enumerate()
        .map(|(i, arg)| generate_arg_parse(arg, i));

    // Generate argument names for method call
    let arg_names: Vec<_> = info.args.iter().map(|arg| &arg.name).collect();

    // Generate result conversion based on return type
    let result_conversion = generate_result_conversion(&info.return_type);

    // Generate completion routing for enum-based arguments using DRY helper
    let enum_completion_arms = info.args.iter().enumerate()
        .filter_map(|(i, arg)| generate_enum_completion_arm(i, arg, &info.completers));

    // Generate arg_index calculation (DRY)
    let arg_index_calc = generate_arg_index_calc();

    // Generate custom completer arms (if any)
    let completion_with_target_arms = info.args.iter().enumerate().filter_map(|(i, arg)| {
        let arg_name_str = arg.name.to_string();
        let completer_method = info.completers.get(&arg_name_str)?;
        Some(generate_custom_completion_arm(i, arg, &info.args, completer_method))
    });

    quote! {
        // Zero-sized command struct (no state!) - private, users don't need to know about it
        #[derive(Default)]
        #[doc(hidden)]
        pub(crate) struct #wrapper_name;

        #[::commands::async_trait::async_trait]
        impl ::commands::AsyncCompleter<#self_type> for #wrapper_name {
            async fn complete(
                &self,
                _full_args: &str,
                context: &::commands::CompletionContext<#self_type>,
            ) -> ::std::result::Result<Vec<::commands::Completion>, ::commands::CompletionError> {
                #arg_index_calc

                // Target is available via context.target
                let target = context.target;

                // Get the partial word being completed
                let partial = context.partial();

                match arg_index {
                    #(#enum_completion_arms)*
                    #(#completion_with_target_arms)*
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

            fn metadata(&self) -> &::commands::CommandMetadata {
                static METADATA: ::commands::CommandMetadata = ::commands::CommandMetadata {
                    name: #command_name,
                    help: #help_text,
                };
                &METADATA
            }
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

    // Get self type for helper method
    let self_type = &input.self_ty;

    // Strip #[command] attributes from the impl block for output
    let cleaned_input = strip_command_attrs(input.clone());

    // Generate wrappers for all commands
    let mut wrappers = Vec::new();
    let mut command_struct_names = Vec::new();

    for (command_name, help_text) in &commands {
        let info = extract_command_info_by_name(&input, command_name, help_text)?;
        let wrapper = generate_command_wrapper(&info);
        wrappers.push(wrapper);

        // Store struct name for registration helper
        let method_name = &info.method_name;
        let struct_name = format_ident!(
            "{}{}Command",
            self_type.to_token_stream().to_string().replace(" ", ""),
            method_name.to_string().to_pascal_case()
        );
        command_struct_names.push(struct_name);
    }

    // Generate Registrable trait impl
    let registrable_impl = quote! {
        impl ::commands::Registrable<::commands::CommandRegistry<#self_type>> for #self_type {
            fn register(registry: &mut ::commands::CommandRegistry<Self>) {
                #(registry.register(#command_struct_names::default());)*
            }
        }
    };

    Ok(quote! {
        #cleaned_input

        #(#wrappers)*

        #registrable_impl
    })
}

/// Implement the #[completer] macro
pub fn impl_completer(input: ImplItem) -> Result<TokenStream> {
    // This is a marker attribute for #[commandable] to detect
    // Just return the original method unchanged
    Ok(quote! {
        #input
    })
}

/// Implement #[command] for a free function
pub fn impl_command_function(func: ItemFn) -> Result<TokenStream> {
    // Extract command metadata from attributes
    let mut command_name = None;
    let mut help_text = String::new();

    for attr in &func.attrs {
        if attr.path().is_ident("command") {
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

    let command_name = command_name.ok_or_else(|| {
        syn::Error::new(func.sig.ident.span(), "Missing command name attribute")
    })?;

    let func_name = &func.sig.ident;
    let return_type = &func.sig.output;

    // Parse arguments - first arg should be &mut T (the target type)
    let mut args = Vec::new();
    let mut target_type = None;

    for (i, arg) in func.sig.inputs.iter().enumerate() {
        if let FnArg::Typed(pat_type) = arg {
            if i == 0 {
                // First arg is the target (&mut T)
                if let Type::Reference(type_ref) = &*pat_type.ty {
                    if type_ref.mutability.is_some() {
                        target_type = Some((*type_ref.elem).clone());
                        continue;
                    }
                }
                return Err(syn::Error::new(
                    pat_type.span(),
                    "First argument must be &mut T (the target type)"
                ));
            }
            args.push(parse_arg_info(pat_type)?);
        }
    }

    let target_type = target_type.ok_or_else(|| {
        syn::Error::new(func.sig.span(), "Function must have &mut T as first parameter")
    })?;

    // Generate wrapper struct name
    let wrapper_name = format_ident!("{}Command", func_name.to_string().to_pascal_case());

    // Generate argument parsing
    let parse_args = args.iter().enumerate().map(|(i, arg)| {
        let arg_name = &arg.name;

        if arg.is_optional {
            if let Some(ref inner_ty) = arg.inner_ty {
                quote! {
                    let #arg_name = args.parse_optional::<#inner_ty>(#i)
                        .map_err(|e| ::commands::CommandError::ParseError(e))?;
                }
            } else {
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

    let arg_names: Vec<_> = args.iter().map(|arg| &arg.name).collect();
    let result_conversion = generate_result_conversion(return_type);

    // Generate completion arms for enum args
    let enum_completion_arms = args.iter().enumerate().filter_map(|(i, arg)| {
        let completion_ty = if arg.is_optional {
            arg.inner_ty.as_ref()?
        } else {
            &arg.ty
        };

        if is_builtin_type(completion_ty) {
            return None;
        }

        Some(quote! {
            #i => {
                let dummy_value = <#completion_ty as ::std::default::Default>::default();
                dummy_value.complete(partial, context).await
            }
        })
    });

    // Keep the original function as-is, just strip the attribute
    let mut cleaned_func = func.clone();
    cleaned_func.attrs.retain(|attr| !attr.path().is_ident("command"));

    Ok(quote! {
        // Keep original function unchanged
        #cleaned_func

        // Generate command wrapper struct
        pub struct #wrapper_name;

        #[::commands::async_trait::async_trait]
        impl ::commands::AsyncCompleter for #wrapper_name {
            type Metadata = ();

            async fn complete(
                &self,
                partial: &str,
                context: &::commands::CompletionContext,
            ) -> ::std::result::Result<Vec<::commands::Completion<()>>, ::commands::CompletionError> {
                let arg_index = if context.input.ends_with(char::is_whitespace) {
                    context.tokens.len().saturating_sub(1)
                } else {
                    context.tokens.len().saturating_sub(2)
                };

                match arg_index {
                    #(#enum_completion_arms)*
                    _ => Ok(vec![])
                }
            }
        }

        #[::commands::async_trait::async_trait]
        impl ::commands::Command<#target_type> for #wrapper_name {
            async fn execute(
                &self,
                target: &mut #target_type,
                args: ::commands::ParsedArgs,
            ) -> ::std::result::Result<::commands::CommandResult, ::commands::CommandError> {
                #(#parse_args)*

                let result = #func_name(target, #(#arg_names),*).await;

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

        // Note: To get the command struct, use the wrapper struct name directly
        // or use the command!() macro helper (to be implemented)
    })
}
