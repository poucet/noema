use syn::{FnArg, Ident, ItemTrait, Pat, ReturnType, TraitItem, Type};

/// Classification of a method's return type.
#[derive(Debug, Clone)]
pub enum ReturnKind {
    /// `-> anyhow::Result<()>` — call_unit
    ResultUnit,
    /// `-> anyhow::Result<T>` — call_val, client deserializes T
    ResultValue { inner: Type },
    /// `-> T` (no Result wrapper) — call_raw, client unwraps_or_default
    RawValue { inner: Type },
    /// `#[rpc(stream)]` `-> Result<(T, S)>` — serialize T, push S to context
    StreamTuple { value_type: Type, stream_type: Type },
    /// `#[rpc(stream)]` `-> Result<S>` — push S, return true
    StreamBare { stream_type: Type },
}

/// Classification of a method's RPC behavior.
#[derive(Debug, Clone, PartialEq)]
pub enum RpcKind {
    /// Normal RPC method — auto-generated dispatch + client.
    Normal,
    /// `#[rpc(stream)]` — returns a stream alongside (or instead of) a value.
    Stream,
    /// `#[rpc(skip)]` — not dispatched, client gets bail!() stub.
    Skip,
}

/// A parsed method parameter (excluding `&self`).
#[derive(Debug, Clone)]
pub struct ParsedParam {
    pub name: Ident,
    /// The type as written in the trait (may be `&T` or `&str`).
    pub ty: Type,
    /// Whether the param is a reference (`&T` or `&str`).
    pub is_ref: bool,
    /// Whether the param is `&str` specifically.
    pub is_str_ref: bool,
    /// The owned type for deserialization (e.g. `String` for `&str`, `T` for `&T`).
    pub owned_type: Type,
}

/// A fully parsed trait method.
#[derive(Debug, Clone)]
pub struct ParsedMethod {
    pub name: Ident,
    pub rpc_kind: RpcKind,
    pub method_name: String,
    pub params: Vec<ParsedParam>,
    pub return_kind: ReturnKind,
    /// The full original method signature (for the client macro to reproduce).
    pub sig: syn::Signature,
}

/// A fully parsed trait.
#[derive(Debug)]
pub struct ParsedTrait {
    pub prefix: String,
    pub trait_name: Ident,
    pub vis: syn::Visibility,
    pub methods: Vec<ParsedMethod>,
}

impl ParsedTrait {
    pub fn from_item_trait(prefix: &str, item: &ItemTrait) -> syn::Result<Self> {
        let mut methods = Vec::new();

        for trait_item in &item.items {
            let TraitItem::Fn(method) = trait_item else { continue };

            let rpc_kind = detect_rpc_kind(&method.attrs);
            let method_name = format!("{}.{}", prefix, method.sig.ident);
            let params = parse_params(&method.sig)?;
            let return_kind = parse_return_type(&method.sig.output, &rpc_kind)?;

            methods.push(ParsedMethod {
                name: method.sig.ident.clone(),
                rpc_kind,
                method_name,
                params,
                return_kind,
                sig: method.sig.clone(),
            });
        }

        Ok(ParsedTrait {
            prefix: prefix.to_string(),
            trait_name: item.ident.clone(),
            vis: item.vis.clone(),
            methods,
        })
    }

    /// Service struct name: e.g. `SessionApiService`
    pub fn service_name(&self) -> Ident {
        Ident::new(
            &format!("{}Service", self.trait_name),
            self.trait_name.span(),
        )
    }

    /// Client macro name: e.g. `impl_remote_session_api`
    pub fn client_macro_name(&self) -> Ident {
        let snake = to_snake_case(&self.trait_name.to_string());
        Ident::new(&format!("impl_remote_{snake}"), self.trait_name.span())
    }

    /// Metadata constant name: e.g. `SESSION_API_META`
    pub fn meta_const_name(&self) -> Ident {
        let upper = to_upper_snake_case(&self.trait_name.to_string());
        Ident::new(&format!("{upper}_META"), self.trait_name.span())
    }
}

/// Detect `#[rpc(skip)]` or `#[rpc(stream)]` on a method.
fn detect_rpc_kind(attrs: &[syn::Attribute]) -> RpcKind {
    for attr in attrs {
        if !attr.path().is_ident("rpc") {
            continue;
        }
        if let Ok(list) = attr.meta.require_list() {
            let tokens = list.tokens.to_string();
            if tokens.contains("skip") {
                return RpcKind::Skip;
            }
            if tokens.contains("stream") {
                return RpcKind::Stream;
            }
        }
    }
    RpcKind::Normal
}

/// Parse method parameters, skipping `&self`.
fn parse_params(sig: &syn::Signature) -> syn::Result<Vec<ParsedParam>> {
    let mut params = Vec::new();

    for arg in &sig.inputs {
        let FnArg::Typed(pat_type) = arg else { continue };
        let Pat::Ident(pat_ident) = pat_type.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &pat_type.pat,
                "rpc_service: only simple ident patterns supported",
            ));
        };

        let name = pat_ident.ident.clone();
        let ty = *pat_type.ty.clone();
        let (is_ref, is_str_ref, owned_type) = analyze_type(&ty);

        params.push(ParsedParam {
            name,
            ty,
            is_ref,
            is_str_ref,
            owned_type,
        });
    }

    Ok(params)
}

/// Determine if a type is a reference and compute its owned form.
fn analyze_type(ty: &Type) -> (bool, bool, Type) {
    if let Type::Reference(ref_type) = ty {
        let inner = &ref_type.elem;
        // Check for &str
        if let Type::Path(path) = inner.as_ref() {
            if path.path.is_ident("str") {
                return (true, true, syn::parse_quote!(String));
            }
        }
        // &T → owned T
        return (true, false, *inner.clone());
    }
    // Owned type
    (false, false, ty.clone())
}

/// Parse the return type and classify it.
fn parse_return_type(output: &ReturnType, rpc_kind: &RpcKind) -> syn::Result<ReturnKind> {
    let ReturnType::Type(_, ty) = output else {
        // No return type — treat as Result<()>
        return Ok(ReturnKind::ResultUnit);
    };

    // Try to extract Result<T> from anyhow::Result<T>
    if let Some(inner) = extract_result_inner(ty) {
        if *rpc_kind == RpcKind::Stream {
            return classify_stream_return(&inner);
        }
        // Check if inner is ()
        if is_unit_type(&inner) {
            return Ok(ReturnKind::ResultUnit);
        }
        return Ok(ReturnKind::ResultValue { inner });
    }

    // No Result wrapper — raw value
    if *rpc_kind == RpcKind::Stream {
        return Err(syn::Error::new_spanned(
            ty,
            "#[rpc(stream)] methods must return anyhow::Result<...>",
        ));
    }
    Ok(ReturnKind::RawValue { inner: *ty.clone() })
}

/// For `#[rpc(stream)]` methods, classify the inner type of Result<...>.
fn classify_stream_return(inner: &Type) -> syn::Result<ReturnKind> {
    // Check for tuple (T, S) — stream returns alongside a value
    if let Type::Tuple(tuple) = inner {
        if tuple.elems.len() == 2 {
            let value_type = tuple.elems[0].clone();
            let stream_type = tuple.elems[1].clone();
            return Ok(ReturnKind::StreamTuple {
                value_type,
                stream_type,
            });
        }
    }
    // Bare stream: Result<S>
    Ok(ReturnKind::StreamBare {
        stream_type: inner.clone(),
    })
}

/// Extract the inner `T` from `Result<T>` or `anyhow::Result<T>`.
fn extract_result_inner(ty: &Type) -> Option<Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let syn::GenericArgument::Type(inner) = args.args.first()? else {
        return None;
    };
    Some(inner.clone())
}

/// Check if a type is `()`.
fn is_unit_type(ty: &Type) -> bool {
    matches!(ty, Type::Tuple(t) if t.elems.is_empty())
}

/// Convert PascalCase to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

/// Convert PascalCase to UPPER_SNAKE_CASE.
fn to_upper_snake_case(s: &str) -> String {
    to_snake_case(s).to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_case() {
        assert_eq!(to_snake_case("SessionApi"), "session_api");
        assert_eq!(to_snake_case("McpApi"), "mcp_api");
        assert_eq!(to_snake_case("OAuthApi"), "o_auth_api");
    }
}
