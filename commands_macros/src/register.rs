use proc_macro2::TokenStream;
use quote::{quote, format_ident};
use syn::{parse::Parser, punctuated::Punctuated, Result, Token};

use crate::ToPascalCase;

pub fn impl_register_commands(input: TokenStream) -> Result<TokenStream> {
    // Parse: registry, Type => [method1, method2, ...]
    let parser = |input: syn::parse::ParseStream| {
        let registry: syn::Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let type_name: syn::Type = input.parse()?;
        input.parse::<Token![=>]>()?;

        let content;
        syn::bracketed!(content in input);
        let methods = Punctuated::<syn::Ident, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();

        Ok((registry, type_name, methods))
    };

    let (registry, type_name, methods): (syn::Expr, syn::Type, Vec<syn::Ident>) = parser.parse2(input)?;

    let registrations = methods.iter().map(|method| {
        // Generate struct name from method name (e.g., cmd_help → AppCmdHelpCommand)
        let struct_name = format_ident!(
            "{}{}Command",
            quote!(#type_name).to_string().replace(" ", ""),
            method.to_string().to_pascal_case()
        );

        quote! {
            #registry.register(#struct_name::default());
        }
    });

    Ok(quote! {
        #(#registrations)*
    })
}
