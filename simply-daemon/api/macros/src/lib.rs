//! `#[skill_router]` — generate an `impl Skill` block from rmcp `#[tool]`-annotated methods.
//!
//! Attach to the same `impl` block that holds your `#[tool]` methods. Generates:
//! - `Skill::name()` from the `name = "..."` arg (or the type name lower-cased)
//! - `Skill::tools()` built from each `#[tool]`-generated `{fn}_tool_attr()`
//! - `Skill::call_tool(name, args, ctx)` dispatching to the matching method, using
//!   `rmcp::handler::server::wrapper::Parameters` to deserialize args and
//!   `rmcp::handler::server::tool::IntoCallToolResult` to normalize return values.
//!
//! Handler signatures may take any combination (in any order, after `&self`) of:
//! - `Parameters<T>` — deserialized tool arguments
//! - `&RequestContext` — the caller's request context (user id + tokens +
//!   origin metadata); forwarded as-is from whoever invoked `call_tool`
//!
//! Works with any return type supported by rmcp's `IntoCallToolResult`: `String`,
//! `impl IntoContents`, `Result<T, E>`, `Json<T>`, `CallToolResult`, etc.
//!
//! Optional attribute args:
//! - `name = "..."` — sets the skill's name (defaults to type ident lower-cased)

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, FnArg, ImplItem, ItemImpl, Meta, Expr, Lit, Type};

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

        // Inspect the signature to plan dispatch — which optional args to forward
        // (Parameters<T> for deserialized args, &RequestContext for caller context).
        let arg_plan = plan_handler_args(&method.sig);

        tools.push(ToolEntry {
            tool_name,
            fn_ident,
            tool_attr_fn,
            arg_plan,
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

    // Build the `call_tool()` dispatch arms — forward the original argument
    // order, supplying Parameters<T> and/or &RequestContext as declared.
    let dispatch_arms = tools.iter().map(|t| {
        let tool_name = &t.tool_name;
        let fn_ident = &t.fn_ident;

        let parse_params = if t.arg_plan.params_ty.is_some() {
            let params_ty = t.arg_plan.params_ty.as_ref().unwrap();
            quote! {
                let parsed: #params_ty = ::simply_daemon_api::__private::serde_json::from_value(arguments)
                    .map_err(|e| ::simply_daemon_api::__private::anyhow::anyhow!("invalid arguments for `{}`: {e}", #tool_name))?;
            }
        } else {
            quote! { let _ = arguments; }
        };

        let call_args: Vec<_> = t.arg_plan.order.iter().map(|kind| match kind {
            HandlerArg::Params => quote! {
                ::simply_daemon_api::__private::rmcp::handler::server::wrapper::Parameters(parsed)
            },
            HandlerArg::Ctx => quote! { _ctx },
        }).collect();

        quote! {
            #tool_name => {
                #parse_params
                let output = self.#fn_ident( #(#call_args),* ).await;
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
                _ctx: &::simply_daemon_api::RequestContext,
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
    arg_plan: ArgPlan,
}

/// What the macro needs to pass to a `#[tool]` handler.
#[derive(Default)]
struct ArgPlan {
    /// Inner type of `Parameters<T>`, if the handler requests deserialized args.
    params_ty: Option<syn::Type>,
    /// The argument kinds in declaration order — used to emit the call in the
    /// same order as the handler signature.
    order: Vec<HandlerArg>,
}

#[derive(Clone, Copy)]
enum HandlerArg {
    Params,
    Ctx,
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

/// Walk a `#[tool]` handler's signature and record which optional arguments
/// the dispatcher must supply, in declaration order.
///
/// Recognised argument shapes (skipping `&self`):
/// - `Parameters<T>` (any path ending in `Parameters`): deserialized args
/// - `&RequestContext` (any path ending in `RequestContext`): caller ctx
///
/// Other arg shapes are ignored — the generated call site only forwards
/// what it knows how to supply.
fn plan_handler_args(sig: &syn::Signature) -> ArgPlan {
    let mut plan = ArgPlan::default();
    for input in &sig.inputs {
        let FnArg::Typed(pat_type) = input else { continue };
        if let Some(inner) = match_parameters_inner(&pat_type.ty) {
            plan.params_ty = Some(inner);
            plan.order.push(HandlerArg::Params);
        } else if is_request_context_ref(&pat_type.ty) {
            plan.order.push(HandlerArg::Ctx);
        }
    }
    plan
}

fn match_parameters_inner(ty: &Type) -> Option<syn::Type> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Parameters" { return None; }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else { return None };
    match args.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner.clone()),
        _ => None,
    }
}

fn is_request_context_ref(ty: &Type) -> bool {
    let Type::Reference(r) = ty else { return false };
    let Type::Path(tp) = &*r.elem else { return false };
    tp.path.segments.last().map(|s| s.ident == "RequestContext").unwrap_or(false)
}
