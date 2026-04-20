//! `#[skill_router]` — generate an `impl Skill` block from rmcp `#[tool]`-annotated methods.
//!
//! Attach to the same `impl` block that holds your `#[tool]` methods. Generates:
//! - `Skill::name()` from the `name = "..."` arg (or the type name lower-cased)
//! - `Skill::tools()` built from each `#[tool]`-generated `{fn}_tool_attr()`
//! - `Skill::call_tool(name, args, ctx)` dispatching to the matching method, using
//!   `rmcp::handler::server::wrapper::Parameters` to deserialize args and
//!   `rmcp::handler::server::tool::IntoCallToolResult` to normalize return values.
//!
//! Works with any return type supported by rmcp's `IntoCallToolResult`: `String`,
//! `impl IntoContents`, `Result<T, E>`, `Json<T>`, `CallToolResult`, etc.
//!
//! Optional attribute args:
//! - `name = "..."` — sets the skill's name (defaults to type ident lower-cased)

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, ImplItem, ItemImpl, Meta, Expr, Lit, Type};

/// Attribute applied to an `impl` block that has `#[tool]`-annotated methods.
#[proc_macro_attribute]
pub fn skill_router(attr: TokenStream, item: TokenStream) -> TokenStream {
    let impl_block = parse_macro_input!(item as ItemImpl);
    let args = parse_macro_input!(attr as SkillArgs);

    // The type this impl is for — used as the name default and in the generated impl.
    let self_ty = &impl_block.self_ty;
    let type_ident = extract_type_ident(self_ty).to_string();
    let skill_name = args.name.unwrap_or_else(|| type_ident.to_lowercase());

    // Walk methods in the impl, find those with #[tool(...)].
    let mut tools: Vec<ToolEntry> = Vec::new();
    for item in &impl_block.items {
        let ImplItem::Fn(method) = item else { continue };
        let tool_attr = method.attrs.iter().find(|a| a.path().is_ident("tool"));
        if tool_attr.is_none() { continue; }

        let fn_ident = method.sig.ident.clone();
        let tool_name = extract_tool_name(tool_attr.unwrap()).unwrap_or_else(|| fn_ident.to_string());
        let tool_attr_fn = format_ident!("{}_tool_attr", fn_ident);

        // Find Parameters<T> in the signature (if any) to drive deserialization.
        let params_ty = find_parameters_type(&method.sig);

        tools.push(ToolEntry {
            tool_name,
            fn_ident,
            tool_attr_fn,
            params_ty,
        });
    }

    // Build the `tools()` impl body: collect each _tool_attr() and convert to ToolDefinition.
    let tool_defs = tools.iter().map(|t| {
        let attr_fn = &t.tool_attr_fn;
        quote! {
            {
                let t = Self::#attr_fn();
                let schema = ::simply_daemon_api::__private::serde_json::to_value(&*t.input_schema).unwrap_or_default();
                ::simply_daemon_api::ToolDefinition {
                    name: t.name.to_string(),
                    description: t.description.as_ref().map(|d| d.to_string()),
                    input_schema: ::simply_daemon_api::__private::serde_json::from_value(schema).unwrap_or_default(),
                }
            }
        }
    });

    // Build the `call_tool()` dispatch arms.
    let dispatch_arms = tools.iter().map(|t| {
        let tool_name = &t.tool_name;
        let fn_ident = &t.fn_ident;
        let invoke = if let Some(params_ty) = &t.params_ty {
            quote! {
                let parsed: #params_ty = ::simply_daemon_api::__private::serde_json::from_value(arguments)
                    .map_err(|e| ::simply_daemon_api::__private::anyhow::anyhow!("invalid arguments for `{}`: {e}", #tool_name))?;
                let fut = self.#fn_ident(::simply_daemon_api::__private::rmcp::handler::server::wrapper::Parameters(parsed));
                fut.await
            }
        } else {
            quote! {
                let _ = arguments;
                let fut = self.#fn_ident();
                fut.await
            }
        };
        quote! {
            #tool_name => {
                let output = { #invoke };
                // rmcp's IntoCallToolResult handles every supported return type:
                // String, impl IntoContents, Result<T, E>, Json<T>, CallToolResult, etc.
                <_ as ::simply_daemon_api::__private::rmcp::handler::server::tool::IntoCallToolResult>::into_call_tool_result(output)
                    .map_err(|e| ::simply_daemon_api::__private::anyhow::anyhow!("tool `{}` failed: {:?}", #tool_name, e))
            }
        }
    });

    let generics = &impl_block.generics;
    let (impl_generics, _, where_clause) = generics.split_for_impl();

    let output = quote! {
        #impl_block

        #[::simply_daemon_api::__async_trait_reexport]
        impl #impl_generics ::simply_daemon_api::Skill for #self_ty #where_clause {
            fn name(&self) -> &str { #skill_name }

            fn tools(&self) -> Vec<::simply_daemon_api::ToolDefinition> {
                vec![ #(#tool_defs),* ]
            }

            async fn call_tool(
                &self,
                name: &str,
                arguments: ::simply_daemon_api::__private::serde_json::Value,
                _ctx: &::simply_daemon_api::SkillCallContext,
            ) -> ::simply_daemon_api::__private::anyhow::Result<::simply_daemon_api::__private::rmcp::model::CallToolResult> {
                match name {
                    #(#dispatch_arms),*
                    other => ::simply_daemon_api::__private::anyhow::bail!("unknown tool: {other}"),
                }
            }
        }
    };

    output.into()
}

// ---------------------------------------------------------------------------
// Attribute arg parsing
// ---------------------------------------------------------------------------

struct SkillArgs {
    name: Option<String>,
}

impl syn::parse::Parse for SkillArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut name = None;
        if input.is_empty() { return Ok(Self { name }); }
        let metas = syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated(input)?;
        for meta in metas {
            if let Meta::NameValue(nv) = meta {
                if nv.path.is_ident("name") {
                    if let Expr::Lit(lit) = nv.value {
                        if let Lit::Str(s) = lit.lit {
                            name = Some(s.value());
                        }
                    }
                }
            }
        }
        Ok(Self { name })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct ToolEntry {
    tool_name: String,
    fn_ident: syn::Ident,
    tool_attr_fn: syn::Ident,
    params_ty: Option<syn::Type>,
}

fn extract_type_ident(ty: &Type) -> syn::Ident {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident.clone();
        }
    }
    syn::Ident::new("Skill", proc_macro2::Span::call_site())
}

/// Extract `name = "..."` from `#[tool(name = "send_message")]`.
fn extract_tool_name(attr: &syn::Attribute) -> Option<String> {
    let Meta::List(list) = &attr.meta else { return None };
    let metas = list.parse_args_with(
        syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
    ).ok()?;
    for meta in metas {
        if let Meta::NameValue(nv) = meta {
            if nv.path.is_ident("name") {
                if let Expr::Lit(lit) = nv.value {
                    if let Lit::Str(s) = lit.lit {
                        return Some(s.value());
                    }
                }
            }
        }
    }
    None
}

/// Find the inner type of `Parameters<T>` in a function's args, if present.
fn find_parameters_type(sig: &syn::Signature) -> Option<syn::Type> {
    for input in &sig.inputs {
        let syn::FnArg::Typed(pat_type) = input else { continue };
        let ty = &*pat_type.ty;
        if let Type::Path(tp) = ty {
            if let Some(seg) = tp.path.segments.last() {
                if seg.ident == "Parameters" {
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                            return Some(inner.clone());
                        }
                    }
                }
            }
        }
    }
    None
}
