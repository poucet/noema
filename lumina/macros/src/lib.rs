use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse_macro_input, FnArg, ItemFn, Lit, Meta, Pat, Type};

/// Derive a slash command from an async function.
///
/// Generates a struct, `SlashCommand` impl, and `inventory` registration.
/// The first two parameters must be `ctx: &Context` and `cmd: &CommandInteraction`.
/// Additional parameters become Discord command options.
///
/// # Attributes
///
/// - `description = "..."` — command description (required)
/// - `#[describe("...")]` on parameters — option description (defaults to param name)
///
/// # Examples
///
/// ```ignore
/// #[slash_command(description = "Check if Lumina is alive")]
/// async fn ping(ctx: &Context, cmd: &CommandInteraction) -> anyhow::Result<()> {
///     reply(ctx, cmd, "Pong!").await
/// }
///
/// #[slash_command(description = "Chat with Lumina")]
/// async fn chat(
///     ctx: &Context,
///     cmd: &CommandInteraction,
///     #[describe("Your message")] message: String,
/// ) -> anyhow::Result<()> {
///     reply(ctx, cmd, &format!("Echo: {message}")).await
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

// ---------------------------------------------------------------------------
// Attribute parsing
// ---------------------------------------------------------------------------

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
                    return Err(syn::Error::new(ident.span(), format!("unknown attribute `{other}`")))
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

// ---------------------------------------------------------------------------
// Parameter info
// ---------------------------------------------------------------------------

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
    // Check for Option<T>
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
                // Strip quotes
                let trimmed = tokens.trim_matches('"');
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

fn generate_command(attrs: CommandAttrs, func: ItemFn) -> syn::Result<TokenStream2> {
    let fn_name = &func.sig.ident;
    let cmd_name = attrs
        .name
        .unwrap_or_else(|| fn_name.to_string().replace('_', "-"));
    let description = &attrs.description;

    // PascalCase struct name from fn name
    let struct_name = format_ident!(
        "{}",
        fn_name
            .to_string()
            .split('_')
            .map(|s| {
                let mut c = s.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            })
            .collect::<String>()
    );

    // Parse parameters (skip first two: ctx, cmd)
    let mut params = Vec::new();
    for (i, arg) in func.sig.inputs.iter().enumerate() {
        if i < 2 {
            continue;
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

    // Generate option registration
    let option_registrations: Vec<TokenStream2> = params
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
        .collect();

    // Generate argument extraction
    let arg_extractions: Vec<TokenStream2> = params
        .iter()
        .map(|p| {
            let var_name = format_ident!("{}", p.name);
            let extract = p.ty.extract_tokens(&p.name);
            if p.required {
                let err_msg = format!("missing required argument `{}`", p.name);
                quote! {
                    let #var_name = opts.iter().find_map(|opt| { #extract })
                        .ok_or_else(|| anyhow::anyhow!(#err_msg))?;
                }
            } else {
                quote! {
                    let #var_name = opts.iter().find_map(|opt| { #extract });
                }
            }
        })
        .collect();

    // Generate call arguments
    let call_args: Vec<TokenStream2> = params
        .iter()
        .map(|p| {
            let var_name = format_ident!("{}", p.name);
            quote! { #var_name }
        })
        .collect();

    // Strip #[describe(...)] attrs from the function params before emitting
    let mut clean_func = func.clone();
    for arg in clean_func.sig.inputs.iter_mut() {
        if let FnArg::Typed(pat_type) = arg {
            pat_type.attrs.retain(|a| !a.path().is_ident("describe"));
        }
    }

    let has_args = !params.is_empty();
    let opts_binding = if has_args {
        quote! { let opts = cmd.data.options(); }
    } else {
        quote! {}
    };

    let cmd_name_lit = &cmd_name;

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
                #cmd_name_lit
            }

            fn register(&self) -> serenity::builder::CreateCommand {
                serenity::builder::CreateCommand::new(#cmd_name_lit)
                    .description(#description)
                    #(#option_registrations)*
            }

            async fn run(
                &self,
                ctx: &serenity::prelude::Context,
                cmd: &serenity::all::CommandInteraction,
            ) -> anyhow::Result<()> {
                #opts_binding
                #(#arg_extractions)*
                #fn_name(ctx, cmd, #(#call_args),*).await
            }
        }
    })
}
