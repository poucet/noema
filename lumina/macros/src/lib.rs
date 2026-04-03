use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse_macro_input, FnArg, ItemFn, ItemMod, Meta, Pat, Type};

// ===========================================================================
// #[slash_command] — standalone command from a function
// ===========================================================================

/// Derive a slash command from an async function.
///
/// ```ignore
/// #[slash_command(description = "Check if Lumina is alive")]
/// async fn ping(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> {
///     cmd.create_response(&lx.http, CreateInteractionResponse::Message(
///         CreateInteractionResponseMessage::new().content("Pong!"),
///     )).await?;
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn slash_command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr as CommandAttrs);
    let func = parse_macro_input!(item as ItemFn);

    match generate_command(attrs, func) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

// ===========================================================================
// #[command_group] — command with subcommands from a module
// ===========================================================================

/// Derive a command group with subcommands from a module.
///
/// Functions inside the module marked with `#[sub_command]` become subcommands.
///
/// ```ignore
/// #[command_group(description = "Chat management")]
/// mod chat {
///     use super::*;
///
///     #[sub_command(description = "Create a new chat channel")]
///     pub async fn new(
///         lx: &LuminaContext,
///         cmd: &CommandInteraction,
///         #[describe("Channel name")] channel_name: Option<String>,
///     ) -> anyhow::Result<()> { ... }
///
///     #[sub_command(description = "Pause bot responses")]
///     pub async fn pause(lx: &LuminaContext, cmd: &CommandInteraction) -> anyhow::Result<()> { ... }
/// }
/// ```
#[proc_macro_attribute]
pub fn command_group(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr as CommandAttrs);
    let module = parse_macro_input!(item as ItemMod);

    match generate_command_group(attrs, module) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Marker attribute for subcommands inside a `#[command_group]` module.
/// Parsed by `#[command_group]`, not used standalone.
#[proc_macro_attribute]
pub fn sub_command(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Identity — the real work is done by #[command_group]
    item
}

// ===========================================================================
// Shared types
// ===========================================================================

struct CommandAttrs {
    description: String,
    name: Option<String>,
}

impl syn::parse::Parse for CommandAttrs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut description = None;
        let mut name = None;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            let lit: syn::LitStr = input.parse()?;

            match ident.to_string().as_str() {
                "description" => description = Some(lit.value()),
                "name" => name = Some(lit.value()),
                other => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!("unknown attribute `{other}`"),
                    ))
                }
            }

            if !input.is_empty() {
                input.parse::<syn::Token![,]>()?;
            }
        }

        let description =
            description.ok_or_else(|| input.error("missing `description = \"...\"`"))?;

        Ok(Self { description, name })
    }
}

struct ParamInfo {
    name: String,
    description: String,
    ty: ParamType,
    required: bool,
}

enum ParamType {
    String,
    Integer,
    Number,
    Boolean,
}

impl ParamType {
    fn option_type_tokens(&self) -> TokenStream2 {
        match self {
            Self::String => quote! { serenity::all::CommandOptionType::String },
            Self::Integer => quote! { serenity::all::CommandOptionType::Integer },
            Self::Number => quote! { serenity::all::CommandOptionType::Number },
            Self::Boolean => quote! { serenity::all::CommandOptionType::Boolean },
        }
    }

    fn extract_tokens(&self, name: &str) -> TokenStream2 {
        let name_str = name;
        match self {
            Self::String => quote! {
                match opt {
                    serenity::all::ResolvedOption {
                        name: #name_str,
                        value: serenity::all::ResolvedValue::String(s),
                        ..
                    } => Some(s.to_string()),
                    _ => None,
                }
            },
            Self::Integer => quote! {
                match opt {
                    serenity::all::ResolvedOption {
                        name: #name_str,
                        value: serenity::all::ResolvedValue::Integer(n),
                        ..
                    } => Some(*n),
                    _ => None,
                }
            },
            Self::Number => quote! {
                match opt {
                    serenity::all::ResolvedOption {
                        name: #name_str,
                        value: serenity::all::ResolvedValue::Number(n),
                        ..
                    } => Some(*n),
                    _ => None,
                }
            },
            Self::Boolean => quote! {
                match opt {
                    serenity::all::ResolvedOption {
                        name: #name_str,
                        value: serenity::all::ResolvedValue::Boolean(b),
                        ..
                    } => Some(*b),
                    _ => None,
                }
            },
        }
    }
}

fn resolve_type(ty: &Type) -> syn::Result<(ParamType, bool)> {
    if let Type::Path(tp) = ty {
        let seg = tp.path.segments.last().unwrap();
        if seg.ident == "Option" {
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    let (pt, _) = resolve_type(inner)?;
                    return Ok((pt, false));
                }
            }
        }
        let name = seg.ident.to_string();
        let pt = match name.as_str() {
            "String" => ParamType::String,
            "str" => ParamType::String,
            "i64" | "i32" | "u64" | "u32" | "usize" | "isize" => ParamType::Integer,
            "f64" | "f32" => ParamType::Number,
            "bool" => ParamType::Boolean,
            _ => {
                return Err(syn::Error::new_spanned(
                    ty,
                    format!("unsupported parameter type `{name}` — use String, i64, f64, or bool"),
                ))
            }
        };
        Ok((pt, true))
    } else {
        Err(syn::Error::new_spanned(ty, "unsupported parameter type"))
    }
}

fn get_describe_attr(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("describe") {
            if let Meta::List(list) = &attr.meta {
                let tokens = list.tokens.to_string();
                let trimmed = tokens.trim_matches('"');
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|seg| {
            let mut c = seg.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

fn parse_params(func: &ItemFn) -> syn::Result<Vec<ParamInfo>> {
    let mut params = Vec::new();
    for (i, arg) in func.sig.inputs.iter().enumerate() {
        if i < 2 {
            continue; // skip lx, cmd
        }
        if let FnArg::Typed(pat_type) = arg {
            let param_name = if let Pat::Ident(pi) = pat_type.pat.as_ref() {
                pi.ident.to_string()
            } else {
                return Err(syn::Error::new_spanned(&pat_type.pat, "expected identifier"));
            };

            let (param_type, required) = resolve_type(&pat_type.ty)?;
            let desc =
                get_describe_attr(&pat_type.attrs).unwrap_or_else(|| param_name.replace('_', " "));

            params.push(ParamInfo {
                name: param_name,
                description: desc,
                ty: param_type,
                required,
            });
        }
    }
    Ok(params)
}

fn gen_option_registrations(params: &[ParamInfo]) -> Vec<TokenStream2> {
    params
        .iter()
        .map(|p| {
            let name = &p.name;
            let desc = &p.description;
            let opt_type = p.ty.option_type_tokens();
            let required = p.required;
            quote! {
                .add_sub_option(
                    serenity::builder::CreateCommandOption::new(#opt_type, #name, #desc)
                        .required(#required)
                )
            }
        })
        .collect()
}

fn gen_top_level_option_registrations(params: &[ParamInfo]) -> Vec<TokenStream2> {
    params
        .iter()
        .map(|p| {
            let name = &p.name;
            let desc = &p.description;
            let opt_type = p.ty.option_type_tokens();
            let required = p.required;
            quote! {
                .add_option(
                    serenity::builder::CreateCommandOption::new(#opt_type, #name, #desc)
                        .required(#required)
                )
            }
        })
        .collect()
}

fn gen_arg_extractions(params: &[ParamInfo]) -> Vec<TokenStream2> {
    params
        .iter()
        .map(|p| {
            let var_name = format_ident!("{}", p.name);
            let extract = p.ty.extract_tokens(&p.name);
            if p.required {
                let err_msg = format!("missing required argument `{}`", p.name);
                quote! {
                    let #var_name = __opts.iter().find_map(|opt| { #extract })
                        .ok_or_else(|| anyhow::anyhow!(#err_msg))?;
                }
            } else {
                quote! {
                    let #var_name = __opts.iter().find_map(|opt| { #extract });
                }
            }
        })
        .collect()
}

fn gen_call_args(params: &[ParamInfo]) -> Vec<TokenStream2> {
    params
        .iter()
        .map(|p| {
            let var_name = format_ident!("{}", p.name);
            quote! { #var_name }
        })
        .collect()
}

fn strip_describe_attrs(func: &mut ItemFn) {
    for arg in func.sig.inputs.iter_mut() {
        if let FnArg::Typed(pat_type) = arg {
            pat_type.attrs.retain(|a| !a.path().is_ident("describe"));
        }
    }
}

// ===========================================================================
// #[slash_command] codegen
// ===========================================================================

fn generate_command(attrs: CommandAttrs, func: ItemFn) -> syn::Result<TokenStream2> {
    let fn_name = &func.sig.ident;
    let cmd_name = attrs
        .name
        .unwrap_or_else(|| fn_name.to_string().replace('_', "-"));
    let description = &attrs.description;
    let struct_name = format_ident!("{}", to_pascal_case(&fn_name.to_string()));

    let params = parse_params(&func)?;
    let option_registrations = gen_top_level_option_registrations(&params);
    let arg_extractions = gen_arg_extractions(&params);
    let call_args = gen_call_args(&params);

    let mut clean_func = func.clone();
    strip_describe_attrs(&mut clean_func);

    let opts_binding = if !params.is_empty() {
        quote! { let __opts = cmd.data.options(); }
    } else {
        quote! {}
    };

    Ok(quote! {
        #clean_func

        #[derive(Default)]
        pub struct #struct_name;

        ::inventory::submit!(crate::commands::CommandInit {
            init: || Box::new(#struct_name),
        });

        #[async_trait::async_trait]
        impl crate::commands::SlashCommand for #struct_name {
            fn name(&self) -> &'static str {
                #cmd_name
            }

            fn register(&self) -> serenity::builder::CreateCommand {
                serenity::builder::CreateCommand::new(#cmd_name)
                    .description(#description)
                    #(#option_registrations)*
            }

            async fn run(
                &self,
                lx: &crate::commands::LuminaContext,
                cmd: &serenity::all::CommandInteraction,
            ) -> anyhow::Result<()> {
                #opts_binding
                #(#arg_extractions)*
                #fn_name(lx, cmd, #(#call_args),*).await
            }
        }
    })
}

// ===========================================================================
// #[mcp_tools] — generate MCP tool definitions + dispatch from annotated impl
// ===========================================================================

/// Marker attribute for MCP tool methods inside an `#[mcp_tools]` impl block.
/// Parsed by `#[mcp_tools]`, not used standalone.
#[proc_macro_attribute]
pub fn mcp_tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Identity — the real work is done by #[mcp_tools]
    item
}

/// Generate MCP tool definitions and dispatch from annotated methods.
///
/// Methods annotated with `#[mcp_tool(description = "...")]` are registered as
/// MCP tools. Parameter types must implement `McpParam` (provides JSON schema,
/// description, and deserialization). `Option<T>` marks a parameter as optional.
///
/// Generates:
/// - `mcp_definitions() -> Vec<Tool>` — tool schemas from function signatures
/// - `mcp_dispatch(&self, name, args) -> Result<Value>` — routes calls to handlers
///
/// ```ignore
/// #[mcp_tools]
/// impl MyServer {
///     #[mcp_tool(description = "List all channels in a guild")]
///     async fn list_channels(&self, guild_id: GuildId) -> Result<Value> {
///         // ...
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn mcp_tools(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let impl_block = parse_macro_input!(item as syn::ItemImpl);
    match generate_mcp_tools(impl_block) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

struct McpToolInfo {
    method_name: syn::Ident,
    tool_name: String,
    description: String,
    params: Vec<McpParamInfo>,
}

struct McpParamInfo {
    name: String,
    /// The concrete type (inner type if Option<T>)
    type_tokens: TokenStream2,
    required: bool,
    description: Option<String>,
}

fn parse_mcp_param_type(ty: &Type) -> (TokenStream2, bool) {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            if seg.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return (quote! { #inner }, false);
                    }
                }
            }
        }
    }
    (quote! { #ty }, true)
}

fn parse_mcp_tools_from_method(method: &syn::ImplItemFn) -> syn::Result<McpToolInfo> {
    let mcp_attr = method
        .attrs
        .iter()
        .find(|a| a.path().is_ident("mcp_tool"))
        .ok_or_else(|| syn::Error::new_spanned(&method.sig.ident, "missing #[mcp_tool]"))?;

    let attrs: CommandAttrs = mcp_attr.parse_args()?;
    let method_name = method.sig.ident.clone();
    let tool_name = attrs.name.unwrap_or_else(|| method_name.to_string());

    let mut params = Vec::new();
    for arg in method.sig.inputs.iter().skip(1) {
        // skip &self
        if let FnArg::Typed(pat_type) = arg {
            let param_name = if let Pat::Ident(pi) = pat_type.pat.as_ref() {
                pi.ident.to_string()
            } else {
                return Err(syn::Error::new_spanned(&pat_type.pat, "expected identifier"));
            };

            let (type_tokens, required) = parse_mcp_param_type(&pat_type.ty);
            let description = get_describe_attr(&pat_type.attrs);

            params.push(McpParamInfo {
                name: param_name,
                type_tokens,
                required,
                description,
            });
        }
    }

    Ok(McpToolInfo {
        method_name,
        tool_name,
        description: attrs.description,
        params,
    })
}

fn gen_mcp_tool_definition(tool: &McpToolInfo) -> TokenStream2 {
    let name = &tool.tool_name;
    let desc = &tool.description;

    let prop_entries: Vec<TokenStream2> = tool
        .params
        .iter()
        .map(|p| {
            let param_name = &p.name;
            let ty = &p.type_tokens;

            if let Some(desc) = &p.description {
                quote! {
                    {
                        let mut s = <#ty as crate::mcp::McpParam>::schema();
                        if let serde_json::Value::Object(ref mut m) = s {
                            m.insert("description".into(), serde_json::Value::String(#desc.into()));
                        }
                        __props.insert(#param_name.into(), s);
                    }
                }
            } else {
                quote! {
                    {
                        let mut s = <#ty as crate::mcp::McpParam>::schema();
                        let d = <#ty as crate::mcp::McpParam>::default_description();
                        if !d.is_empty() {
                            if let serde_json::Value::Object(ref mut m) = s {
                                m.insert("description".into(), serde_json::Value::String(d.into()));
                            }
                        }
                        __props.insert(#param_name.into(), s);
                    }
                }
            }
        })
        .collect();

    let required_names: Vec<&str> = tool
        .params
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.as_str())
        .collect();

    quote! {
        {
            let mut __props = serde_json::Map::new();
            #(#prop_entries)*

            let mut __schema = serde_json::Map::new();
            __schema.insert("type".into(), serde_json::Value::String("object".into()));
            __schema.insert("properties".into(), serde_json::Value::Object(__props));
            __schema.insert("required".into(), serde_json::json!([#(#required_names),*]));

            rmcp::model::Tool {
                name: #name.into(),
                title: None,
                description: Some(#desc.into()),
                input_schema: std::sync::Arc::new(__schema),
                annotations: None,
                output_schema: None,
                icons: None,
                meta: None,
            }
        }
    }
}

fn gen_mcp_dispatch_wrapper(tool: &McpToolInfo) -> TokenStream2 {
    let method_name = &tool.method_name;
    let wrapper_name = format_ident!("__mcp_dispatch_{}", method_name);

    let extractions: Vec<TokenStream2> = tool
        .params
        .iter()
        .map(|p| {
            let var = format_ident!("{}", p.name);
            let param_name = &p.name;
            let ty = &p.type_tokens;
            let err = format!("missing required parameter: {}", p.name);

            if p.required {
                quote! {
                    let #var = <#ty as crate::mcp::McpParam>::from_value(
                        __args.get(#param_name).ok_or_else(|| anyhow::anyhow!(#err))?
                    )?;
                }
            } else {
                quote! {
                    let #var = __args.get(#param_name)
                        .map(|v| <#ty as crate::mcp::McpParam>::from_value(v))
                        .transpose()?;
                }
            }
        })
        .collect();

    let call_args: Vec<TokenStream2> = tool
        .params
        .iter()
        .map(|p| {
            let var = format_ident!("{}", p.name);
            quote! { #var }
        })
        .collect();

    quote! {
        async fn #wrapper_name(
            &self,
            __args: serde_json::Map<String, serde_json::Value>,
        ) -> anyhow::Result<serde_json::Value> {
            #(#extractions)*
            self.#method_name(#(#call_args),*).await
        }
    }
}

fn generate_mcp_tools(mut impl_block: syn::ItemImpl) -> syn::Result<TokenStream2> {
    let self_ty = &impl_block.self_ty;

    // Collect tool info from annotated methods
    let mut tools = Vec::new();
    for item in &impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            if method.attrs.iter().any(|a| a.path().is_ident("mcp_tool")) {
                tools.push(parse_mcp_tools_from_method(method)?);
            }
        }
    }

    // Strip #[mcp_tool] and #[describe] attributes
    for item in &mut impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            method.attrs.retain(|a| !a.path().is_ident("mcp_tool"));
            for arg in method.sig.inputs.iter_mut() {
                if let FnArg::Typed(pat_type) = arg {
                    pat_type.attrs.retain(|a| !a.path().is_ident("describe"));
                }
            }
        }
    }

    let tool_defs: Vec<TokenStream2> = tools.iter().map(gen_mcp_tool_definition).collect();
    let dispatch_wrappers: Vec<TokenStream2> = tools.iter().map(gen_mcp_dispatch_wrapper).collect();

    let dispatch_arms: Vec<TokenStream2> = tools
        .iter()
        .map(|t| {
            let name = &t.tool_name;
            let wrapper = format_ident!("__mcp_dispatch_{}", t.method_name);
            quote! { #name => self.#wrapper(__args).await }
        })
        .collect();

    Ok(quote! {
        #impl_block

        impl #self_ty {
            pub fn mcp_definitions() -> Vec<rmcp::model::Tool> {
                vec![#(#tool_defs),*]
            }

            pub async fn mcp_dispatch(
                &self,
                __name: &str,
                __args: serde_json::Map<String, serde_json::Value>,
            ) -> anyhow::Result<serde_json::Value> {
                match __name {
                    #(#dispatch_arms,)*
                    _ => Err(anyhow::anyhow!("unknown tool: {}", __name)),
                }
            }

            #(#dispatch_wrappers)*
        }
    })
}

// ===========================================================================
// #[command_group] codegen
// ===========================================================================

struct SubCommandInfo {
    fn_name: syn::Ident,
    sub_name: String,
    description: String,
    params: Vec<ParamInfo>,
}

fn generate_command_group(attrs: CommandAttrs, module: ItemMod) -> syn::Result<TokenStream2> {
    let mod_name = &module.ident;
    let cmd_name = attrs
        .name
        .unwrap_or_else(|| mod_name.to_string().replace('_', "-"));
    let description = &attrs.description;
    let struct_name = format_ident!("{}", to_pascal_case(&mod_name.to_string()));

    let items = module
        .content
        .as_ref()
        .map(|(_, items)| items.as_slice())
        .unwrap_or(&[]);

    // Find all #[sub_command] functions
    let mut sub_commands: Vec<SubCommandInfo> = Vec::new();
    for item in items {
        if let syn::Item::Fn(func) = item {
            let sub_attr = func
                .attrs
                .iter()
                .find(|a| a.path().is_ident("sub_command"));
            if let Some(attr) = sub_attr {
                let sub_attrs: CommandAttrs = attr.parse_args()?;
                let params = parse_params(func)?;
                let sub_name = sub_attrs
                    .name
                    .unwrap_or_else(|| func.sig.ident.to_string().replace('_', "-"));
                sub_commands.push(SubCommandInfo {
                    fn_name: func.sig.ident.clone(),
                    sub_name,
                    description: sub_attrs.description,
                    params,
                });
            }
        }
    }

    if sub_commands.is_empty() {
        return Err(syn::Error::new_spanned(
            &module.ident,
            "command_group requires at least one #[sub_command] function",
        ));
    }

    // Emit cleaned module (strip #[sub_command] and #[describe] attrs)
    let mut clean_module = module.clone();
    if let Some((_, ref mut items)) = clean_module.content {
        for item in items.iter_mut() {
            if let syn::Item::Fn(func) = item {
                func.attrs.retain(|a| !a.path().is_ident("sub_command"));
                strip_describe_attrs(func);
            }
        }
    }

    // Generate subcommand option definitions for register()
    let sub_options: Vec<TokenStream2> = sub_commands
        .iter()
        .map(|sc| {
            let sub_name = &sc.sub_name;
            let sub_desc = &sc.description;
            let param_opts = gen_option_registrations(&sc.params);
            quote! {
                .add_option(
                    serenity::builder::CreateCommandOption::new(
                        serenity::all::CommandOptionType::SubCommand,
                        #sub_name,
                        #sub_desc,
                    )
                    #(#param_opts)*
                )
            }
        })
        .collect();

    // Generate dispatch match arms for run()
    let match_arms: Vec<TokenStream2> = sub_commands
        .iter()
        .map(|sc| {
            let sub_name = &sc.sub_name;
            let fn_name = &sc.fn_name;
            let arg_extractions = gen_arg_extractions(&sc.params);
            let call_args = gen_call_args(&sc.params);
            quote! {
                #sub_name => {
                    if let serenity::all::ResolvedValue::SubCommand(ref __opts) = __sub.value {
                        #(#arg_extractions)*
                        #mod_name::#fn_name(lx, cmd, #(#call_args),*).await
                    } else {
                        Err(anyhow::anyhow!("expected subcommand value"))
                    }
                }
            }
        })
        .collect();

    Ok(quote! {
        #clean_module

        #[derive(Default)]
        pub struct #struct_name;

        ::inventory::submit!(crate::commands::CommandInit {
            init: || Box::new(#struct_name),
        });

        #[async_trait::async_trait]
        impl crate::commands::SlashCommand for #struct_name {
            fn name(&self) -> &'static str {
                #cmd_name
            }

            fn register(&self) -> serenity::builder::CreateCommand {
                serenity::builder::CreateCommand::new(#cmd_name)
                    .description(#description)
                    #(#sub_options)*
            }

            async fn run(
                &self,
                lx: &crate::commands::LuminaContext,
                cmd: &serenity::all::CommandInteraction,
            ) -> anyhow::Result<()> {
                let __opts = cmd.data.options();
                let __sub = __opts.first()
                    .ok_or_else(|| anyhow::anyhow!("missing subcommand"))?;
                match __sub.name {
                    #(#match_arms)*
                    other => Err(anyhow::anyhow!("unknown subcommand `{other}`")),
                }
            }
        }
    })
}
