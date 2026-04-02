use proc_macro::TokenStream;

mod parse;
mod codegen_dispatch;
mod codegen_client;

/// Annotate an `async_trait` to auto-generate RPC server dispatch + client impl.
///
/// ```rust,ignore
/// #[rpc_service("session")]
/// #[async_trait]
/// pub trait SessionApi: Send + Sync {
///     async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>>;
///
///     #[rpc(stream)]
///     async fn create_session(&self, opts: CreateSessionOptions)
///         -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)>;
///
///     #[rpc(skip)]
///     async fn voice_connect(&self, session_id: &SessionId) -> anyhow::Result<VoiceHandle>;
/// }
/// ```
///
/// Generates:
/// - `SessionApiService<T>` implementing `simply_rpc::RpcService<S>`
/// - `impl_remote_session_api!($T)` macro implementing the trait for any `simply_rpc::RpcClient`
#[proc_macro_attribute]
pub fn rpc_service(attr: TokenStream, item: TokenStream) -> TokenStream {
    let prefix = syn::parse_macro_input!(attr as syn::LitStr).value();
    let input = syn::parse_macro_input!(item as syn::ItemTrait);

    match generate(prefix, input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn generate(
    prefix: String,
    mut input: syn::ItemTrait,
) -> syn::Result<proc_macro2::TokenStream> {
    let parsed = parse::ParsedTrait::from_item_trait(&prefix, &input)?;
    let dispatch = codegen_dispatch::generate(&parsed)?;
    let client_macro = codegen_client::generate(&parsed)?;

    // Strip #[rpc(...)] attributes from the trait before passing through
    // (they're only meaningful to us, not to rustc or async_trait)
    for item in &mut input.items {
        if let syn::TraitItem::Fn(method) = item {
            method.attrs.retain(|attr| !attr.path().is_ident("rpc"));
        }
    }

    let original = quote::quote! { #input };

    Ok(quote::quote! {
        #original
        #dispatch
        #client_macro
    })
}
