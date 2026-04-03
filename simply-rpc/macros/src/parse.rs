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

/// An HTTP method (or WebSocket upgrade) for routing.
#[derive(Debug, Clone, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    /// WebSocket upgrade — `#[rpc(stream = "/path")]`.
    Stream,
}

/// Endpoint info parsed from annotations like `#[rpc(get = "/path/{param}")]`
/// or `#[rpc(stream = "/path")]`.
#[derive(Debug, Clone)]
pub struct RestEndpoint {
    pub http_method: HttpMethod,
    pub path_template: String,
    /// Parameter names extracted from `{name}` segments in the path.
    pub path_params: Vec<String>,
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
    /// Parameter names that should be base64-encoded over the wire (`#[rpc(base64_param = "name")]`).
    pub base64_params: Vec<String>,
    /// Whether the return value should be base64-encoded (`#[rpc(base64_return)]`).
    pub base64_return: bool,
    /// Expose as HTTP GET endpoint (`#[rpc(rest_get)]`) — legacy, prefer `rest_endpoint`.
    pub rest_get: bool,
    /// Endpoint info parsed from `#[rpc(get = "/path")]`, `#[rpc(stream = "/path")]`, etc.
    pub rest_endpoint: Option<RestEndpoint>,
    /// Whether to exclude this method from tool generation (`#[rpc(no_tool)]`).
    pub no_tool: bool,
    /// Response is immutably cacheable (`#[rpc(immutable_cache)]`). Adds Cache-Control + ETag headers.
    pub immutable_cache: bool,
    /// Doc comment extracted from `///` attributes.
    pub doc_comment: Option<String>,
    pub method_name: String,
    pub params: Vec<ParsedParam>,
    pub return_kind: ReturnKind,
    /// The full original method signature (for the client macro to reproduce).
    pub sig: syn::Signature,
}

impl ParsedMethod {
    /// Check if a parameter should be base64-encoded.
    pub fn is_base64_param(&self, name: &str) -> bool {
        self.base64_params.iter().any(|p| p == name)
    }
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

            let rpc_attrs = detect_rpc_attrs(&method.attrs);
            let doc_comment = extract_doc_comment(&method.attrs);
            let method_name = format!("{}.{}", prefix, method.sig.ident);
            let params = parse_params(&method.sig)?;
            let return_kind = parse_return_type(&method.sig.output, &rpc_attrs.kind)?;

            methods.push(ParsedMethod {
                name: method.sig.ident.clone(),
                rpc_kind: rpc_attrs.kind,
                base64_params: rpc_attrs.base64_params,
                base64_return: rpc_attrs.base64_return,
                rest_get: rpc_attrs.rest_get,
                rest_endpoint: rpc_attrs.rest_endpoint,
                no_tool: rpc_attrs.no_tool,
                immutable_cache: rpc_attrs.immutable_cache,
                doc_comment,
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

/// Parsed `#[rpc(...)]` attributes.
struct RpcAttrs {
    kind: RpcKind,
    base64_params: Vec<String>,
    base64_return: bool,
    rest_get: bool,
    rest_endpoint: Option<RestEndpoint>,
    no_tool: bool,
    immutable_cache: bool,
}

/// Extract `{name}` path parameters from a path template like `/session/{session_id}/message`.
fn extract_path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        if let Some(end) = rest[start..].find('}') {
            params.push(rest[start + 1..start + end].to_string());
            rest = &rest[start + end + 1..];
        } else {
            break;
        }
    }
    params
}

/// Try to parse a route annotation like `get = "/path"` or `stream = "/path"`.
fn try_parse_rest(key: &str, value: &str) -> Option<RestEndpoint> {
    let http_method = match key {
        "get" => HttpMethod::Get,
        "post" => HttpMethod::Post,
        "put" => HttpMethod::Put,
        "delete" => HttpMethod::Delete,
        "stream" => HttpMethod::Stream,
        _ => return None,
    };
    let path_params = extract_path_params(value);
    Some(RestEndpoint {
        http_method,
        path_template: value.to_string(),
        path_params,
    })
}

/// Detect `#[rpc(...)]` attributes on a method.
///
/// Supported: `skip`, `stream`, `base64_param = "name"`, `base64_return`, `rest_get`,
/// `get = "/path"`, `post = "/path"`, `put = "/path"`, `delete = "/path"`, `no_tool`
fn detect_rpc_attrs(attrs: &[syn::Attribute]) -> RpcAttrs {
    let mut result = RpcAttrs {
        kind: RpcKind::Normal,
        base64_params: Vec::new(),
        base64_return: false,
        rest_get: false,
        rest_endpoint: None,
        no_tool: false,
        immutable_cache: false,
    };

    for attr in attrs {
        if !attr.path().is_ident("rpc") {
            continue;
        }
        if let Ok(list) = attr.meta.require_list() {
            let tokens = list.tokens.to_string();
            if tokens.contains("skip") {
                result.kind = RpcKind::Skip;
            }
            // Bare `stream` (no path) — backward compat
            {
                let token_str_check = tokens.replace(' ', "");
                if token_str_check.split(',').any(|s| s.trim() == "stream") {
                    result.kind = RpcKind::Stream;
                }
            }
            if tokens.contains("base64_return") {
                result.base64_return = true;
            }
            if tokens.contains("rest_get") {
                result.rest_get = true;
            }
            if tokens.contains("no_tool") {
                result.no_tool = true;
            }
            if tokens.contains("immutable_cache") {
                result.immutable_cache = true;
            }
            let token_str = tokens.replace(' ', "");
            for segment in token_str.split(',') {
                let segment = segment.trim();
                // Parse base64_param = "name"
                if let Some(value) = segment.strip_prefix("base64_param=") {
                    let name = value.trim_matches('"');
                    result.base64_params.push(name.to_string());
                }
                // Parse route annotations: get="/path", post="/path", stream="/path", etc.
                for method_key in &["get", "post", "put", "delete", "stream"] {
                    if let Some(value) = segment.strip_prefix(&format!("{method_key}=")) {
                        let path = value.trim_matches('"');
                        if let Some(endpoint) = try_parse_rest(method_key, path) {
                            if endpoint.http_method == HttpMethod::Stream {
                                result.kind = RpcKind::Stream;
                            }
                            result.rest_endpoint = Some(endpoint);
                        }
                    }
                }
            }
        }
    }

    result
}

/// Extract `///` doc comment lines from attributes, joined into a single string.
fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    let lines: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &nv.value {
                    if let syn::Lit::Str(s) = &expr_lit.lit {
                        return Some(s.value().trim().to_string());
                    }
                }
            }
            None
        })
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
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

/// Convert snake_case to PascalCase.
pub fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
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
