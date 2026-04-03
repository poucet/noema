//! Tests for #[rpc(base64)] — binary data encoded as base64 over the wire.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use simply_rpc::{HttpMethod, RestDispatcher, RpcClient, RpcService};

// ---------------------------------------------------------------------------
// Test trait with base64 methods
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlobId(pub String);

impl std::fmt::Display for BlobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[simply_rpc::rpc_service("blob")]
#[async_trait]
pub trait BlobApi: Send + Sync {
    /// Store binary data — `data` param encoded as base64 over the wire.
    #[rpc(post = "/blob", base64_param = "data")]
    async fn store_blob(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<BlobId>;

    /// Get binary data — return value encoded as base64 over the wire.
    #[rpc(get = "/blob/{id}", base64_return)]
    async fn get_blob(&self, id: &str) -> anyhow::Result<Vec<u8>>;
}

// ---------------------------------------------------------------------------
// In-memory implementation
// ---------------------------------------------------------------------------

struct InMemoryBlobs {
    blobs: tokio::sync::Mutex<Vec<(BlobId, String, Vec<u8>)>>,
}

impl InMemoryBlobs {
    fn new() -> Self {
        Self { blobs: tokio::sync::Mutex::new(Vec::new()) }
    }
}

#[async_trait]
impl BlobApi for InMemoryBlobs {
    async fn store_blob(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<BlobId> {
        let id = BlobId(format!("blob_{}", self.blobs.lock().await.len()));
        self.blobs.lock().await.push((id.clone(), media_type.to_string(), data));
        Ok(id)
    }

    async fn get_blob(&self, id: &str) -> anyhow::Result<Vec<u8>> {
        self.blobs.lock().await
            .iter()
            .find(|(bid, _, _)| bid.0 == id)
            .map(|(_, _, data)| data.clone())
            .ok_or_else(|| anyhow::anyhow!("not found: {id}"))
    }
}

fn make_rd() -> (RestDispatcher, Arc<InMemoryBlobs>) {
    let impl_ = Arc::new(InMemoryBlobs::new());
    let svc = <dyn BlobApi>::service(impl_.clone());
    (RestDispatcher::new().register(svc), impl_)
}

// ---------------------------------------------------------------------------
// REST dispatch tests — base64 encoding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_store_blob_decodes_base64_param() {
    let (rd, _impl) = make_rd();

    let b64_data = simply_rpc::encode_base64(b"hello world");
    let params = serde_json::json!({"data": b64_data, "media_type": "text/plain"});

    let result = rd.dispatch(HttpMethod::Post, "/blob", params).await;
    let id: BlobId = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(id.0, "blob_0");
}

#[tokio::test]
async fn dispatch_get_blob_encodes_base64_return() {
    let (rd, impl_) = make_rd();

    impl_.store_blob(b"binary data".to_vec(), "application/octet-stream").await.unwrap();

    let result = rd.dispatch(HttpMethod::Get, "/blob/blob_0", Value::Null).await;
    let b64: String = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    let decoded = simply_rpc::decode_base64(&b64).unwrap();
    assert_eq!(decoded, b"binary data");
}

// ---------------------------------------------------------------------------
// Client round-trip tests — base64 transparent to caller
// ---------------------------------------------------------------------------

struct MockBlobClient {
    rd: RestDispatcher,
}

impl MockBlobClient {
    fn new() -> Self {
        let (rd, _) = make_rd();
        Self { rd }
    }
}

#[async_trait]
impl RpcClient for MockBlobClient {
    type Stream = ();

    async fn rpc_call(&self, method: &str, _params: Value) -> anyhow::Result<Value> {
        Err(anyhow::anyhow!("rpc_call should not be used for REST methods: {method}"))
    }

    async fn register_stream(&self, _id: &str) -> Self::Stream {}
    async fn unregister_stream(&self, _id: &str) {}

    async fn rest_call(
        &self,
        http_method: HttpMethod,
        path: &str,
        body: Value,
    ) -> anyhow::Result<Value> {
        match self.rd.dispatch(http_method, path, body).await {
            Some(result) => result,
            None => Err(anyhow::anyhow!("no REST handler for path: {path}")),
        }
    }
}

impl_remote_blob_api!(MockBlobClient);

#[tokio::test]
async fn client_store_and_get_blob_round_trip() {
    let client = MockBlobClient::new();

    let original = b"some binary content \x00\x01\x02".to_vec();
    let id = client.store_blob(original.clone(), "application/octet-stream").await.unwrap();

    let retrieved = client.get_blob(&id.0).await.unwrap();
    assert_eq!(retrieved, original);
}

#[tokio::test]
async fn client_store_blob_empty_data() {
    let client = MockBlobClient::new();

    let id = client.store_blob(vec![], "text/plain").await.unwrap();
    let retrieved = client.get_blob(&id.0).await.unwrap();
    assert!(retrieved.is_empty());
}

#[tokio::test]
async fn client_get_blob_not_found() {
    let client = MockBlobClient::new();

    let result = client.get_blob("nonexistent").await;
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Metadata tests
// ---------------------------------------------------------------------------

#[test]
fn rest_meta_for_base64_methods() {
    let meta = &BLOB_API_META;
    assert_eq!(meta.routes.len(), 2);

    let store = meta.routes.iter().find(|m| m.method_name == "blob.store_blob").unwrap();
    assert_eq!(store.http_method(), Some(HttpMethod::Post));
    assert_eq!(store.path_template, "/blob");

    let get = meta.routes.iter().find(|m| m.method_name == "blob.get_blob").unwrap();
    assert_eq!(get.http_method(), Some(HttpMethod::Get));
    assert_eq!(get.path_template, "/blob/{id}");
}
