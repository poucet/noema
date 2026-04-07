//! TypeScript API client generator.
//!
//! Scans Rust source files for `#[rpc_service("prefix")]` traits and generates
//! typed TypeScript clients with method signatures, URL construction, and proper
//! HTTP method + body wrapping.
//!
//! Usage: cargo run -p simply-rpc --features codegen --bin rpc-codegen -- <src_dir> <output.ts>

use std::path::Path;
use std::{env, fs};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: rpc-codegen <src_dir> <output.ts>");
        eprintln!("  Scans <src_dir>/**/*.rs for #[rpc_service] traits");
        eprintln!("  Writes generated TypeScript to <output.ts>");
        std::process::exit(1);
    }

    let src_dir = &args[1];
    let output_path = &args[2];

    let pattern = format!("{src_dir}/**/*.rs");
    let files: Vec<_> = glob::glob(&pattern)
        .expect("invalid glob pattern")
        .filter_map(|r| r.ok())
        .collect();

    let mut ts_output = String::new();
    ts_output.push_str("// Auto-generated TypeScript API clients.\n");
    ts_output.push_str("// Do not edit — regenerate with: cargo run -p simply-rpc --features codegen --bin rpc-codegen\n\n");
    ts_output.push_str("import { json, type Fetch } from './runtime';\n");
    ts_output.push_str("// Import ts-rs generated types — run `cargo test -p simply-daemon` to regenerate\n");
    ts_output.push_str("import type * as T from './types';\n\n");

    let mut generated = 0;

    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Quick check before parsing
        if !source.contains("rpc_service") { continue; }

        let syntax = match syn::parse_file(&source) {
            Ok(f) => f,
            Err(_) => continue,
        };

        for item in &syntax.items {
            let syn::Item::Trait(trait_item) = item else { continue };

            // Find #[rpc_service("prefix")] or #[simply_rpc::rpc_service("prefix")]
            let prefix = extract_rpc_service_prefix(&trait_item.attrs);
            let Some(prefix) = prefix else { continue };

            let ts = generate_service_client(&prefix, trait_item);
            ts_output.push_str(&ts);
            ts_output.push('\n');
            generated += 1;
        }
    }

    fs::write(output_path, &ts_output).expect("failed to write output file");
    eprintln!("Generated {generated} API client(s) → {output_path}");
}

fn extract_rpc_service_prefix(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        let path = attr.path();
        let is_rpc = path.is_ident("rpc_service")
            || path.segments.iter().any(|s| s.ident == "rpc_service");
        if !is_rpc { continue; }

        if let syn::Meta::List(list) = &attr.meta {
            let tokens = list.tokens.to_string();
            let prefix = tokens.trim().trim_matches('"').to_string();
            if !prefix.is_empty() {
                return Some(prefix);
            }
        }
    }
    None
}

fn generate_service_client(prefix: &str, trait_item: &syn::ItemTrait) -> String {
    let trait_name = trait_item.ident.to_string();
    let mut out = String::new();

    out.push_str(&format!("// {trait_name} — prefix: /{prefix}\n"));
    out.push_str(&format!(
        "export const {} = (f: Fetch = fetch) => ({{\n",
        to_camel_case(&trait_name)
    ));

    for item in &trait_item.items {
        let syn::TraitItem::Fn(method) = item else { continue };

        let (http_method, path_template) = match extract_rpc_route(&method.attrs) {
            Some(v) => v,
            None => continue,
        };

        // Skip stream methods
        if http_method == "STREAM" { continue; }

        let method_name = method.sig.ident.to_string();
        let doc = extract_doc_comment(&method.attrs);
        let params = parse_method_params(&method.sig);
        let path_params = extract_path_params(&path_template);
        let return_type = parse_return_type_ts(&method.sig.output);

        let mut ts_params = Vec::new();
        let mut body_params = Vec::new();

        for (name, ty) in &params {
            let ts_type = rust_type_str_to_ts(ty);
            ts_params.push(format!("{name}: {ts_type}"));
            if !path_params.contains(name) {
                body_params.push(name.clone());
            }
        }

        let params_str = ts_params.join(", ");
        let url = build_ts_url(prefix, &path_template, &path_params);

        let body = if body_params.is_empty() {
            None
        } else if body_params.len() == 1 && path_params.is_empty() {
            Some(body_params[0].clone())
        } else {
            Some(format!("{{ {} }}", body_params.join(", ")))
        };

        if let Some(ref doc) = doc {
            out.push_str(&format!("  /** {doc} */\n"));
        }

        let fn_name = to_camel_case(&method_name);
        match body {
            None => out.push_str(&format!(
                "  {fn_name}: ({params_str}) => json<{return_type}>(f, '{http_method}', {url}),\n"
            )),
            Some(b) => out.push_str(&format!(
                "  {fn_name}: ({params_str}) => json<{return_type}>(f, '{http_method}', {url}, {b}),\n"
            )),
        }
    }

    out.push_str("});\n");
    out
}

fn extract_rpc_route(attrs: &[syn::Attribute]) -> Option<(String, String)> {
    for attr in attrs {
        if !attr.path().is_ident("rpc") { continue; }
        let syn::Meta::List(list) = &attr.meta else { continue };
        let tokens = list.tokens.to_string().replace(' ', "");

        for method in &["get", "post", "put", "delete", "stream"] {
            if let Some(rest) = tokens.strip_prefix(&format!("{method}=")) {
                let path = rest.split(',').next().unwrap_or(rest).trim_matches('"');
                let http = if *method == "stream" { "STREAM" } else { method.to_uppercase() };
                return Some((http, path.to_string()));
            }
        }

        // Check for skip
        if tokens.contains("skip") { return None; }
    }
    None
}

fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    let lines: Vec<String> = attrs.iter().filter_map(|attr| {
        if !attr.path().is_ident("doc") { return None; }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(lit) = &nv.value {
                if let syn::Lit::Str(s) = &lit.lit {
                    return Some(s.value().trim().to_string());
                }
            }
        }
        None
    }).collect();
    if lines.is_empty() { None } else { Some(lines.join(" ")) }
}

fn parse_method_params(sig: &syn::Signature) -> Vec<(String, String)> {
    sig.inputs.iter().filter_map(|arg| {
        let syn::FnArg::Typed(pat_type) = arg else { return None };
        let syn::Pat::Ident(pat_ident) = pat_type.pat.as_ref() else { return None };
        let name = pat_ident.ident.to_string();
        let ty_ref = &pat_type.ty;
        let ty = quote::quote!(#ty_ref).to_string().replace(' ', "");
        Some((name, ty))
    }).collect()
}

fn parse_return_type_ts(output: &syn::ReturnType) -> String {
    let syn::ReturnType::Type(_, ty) = output else { return "void".to_string() };
    let s = quote::quote!(#ty).to_string().replace(' ', "");

    // Extract inner from anyhow::Result<T> or Result<T>
    let inner = extract_generic_inner(&s, "Result")
        .unwrap_or(s.clone());

    if inner == "()" { return "void".to_string(); }
    rust_type_str_to_ts(&inner)
}

fn extract_generic_inner(s: &str, wrapper: &str) -> Option<String> {
    // Handle both `Result<T>` and `anyhow::Result<T>`
    let needle = format!("{wrapper}<");
    if let Some(start) = s.find(&needle) {
        let rest = &s[start + needle.len()..];
        // Find matching >
        let mut depth = 1;
        let mut end = 0;
        for (i, c) in rest.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => { depth -= 1; if depth == 0 { end = i; break; } }
                _ => {}
            }
        }
        Some(rest[..end].to_string())
    } else {
        None
    }
}

fn extract_path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        if let Some(end) = rest[start..].find('}') {
            params.push(rest[start + 1..start + end].to_string());
            rest = &rest[start + end + 1..];
        } else { break; }
    }
    params
}

fn build_ts_url(prefix: &str, template: &str, path_params: &[String]) -> String {
    if path_params.is_empty() {
        format!("'/api{template}'")
    } else {
        let mut url = format!("/api{template}");
        for param in path_params {
            url = url.replace(&format!("{{{param}}}"), &format!("${{{param}}}"));
        }
        format!("`{url}`")
    }
}

fn rust_type_str_to_ts(s: &str) -> String {
    match s {
        "String" | "&str" | "str" => return "string".to_string(),
        "bool" => return "boolean".to_string(),
        "i32" | "i64" | "u32" | "u64" | "usize" | "isize" | "f32" | "f64" => return "number".to_string(),
        "()" => return "void".to_string(),
        _ => {}
    }

    if let Some(inner) = extract_generic_inner(s, "Vec") {
        return format!("{}[]", rust_type_str_to_ts(&inner));
    }
    if let Some(inner) = extract_generic_inner(s, "Option") {
        return format!("{} | null", rust_type_str_to_ts(&inner));
    }

    // Complex type — reference ts-rs generated type via T. prefix
    let name = s.rsplit("::").next().unwrap_or(s);
    format!("T.{name}")
}

fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for (i, c) in s.chars().enumerate() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else if i == 0 {
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}
