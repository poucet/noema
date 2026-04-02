//! Tests for #[rpc(base64)] — binary data encoded as base64 over the wire.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simply_rpc::{RpcClient, RpcService};

// ---------------------------------------------------------------------------
// Test trait with base64 methods
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlobId(pub String);

impl BlobId {
    pub fn as_str(&self) -> &str { &self.0 }
}

#[simply_rpc::rpc_service("blob")]
#[async_trait]
pub trait BlobApi: Send + Sync {
    /// Store binary data — `data` param encoded as base64 over the wire.
    #[rpc(base64_param = "data")]
    async fn store_blob(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<BlobId>;

    /// Get binary data — return value encoded as base64 over the wire.
    #[rpc(base64_return)]
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

// ---------------------------------------------------------------------------
// Server dispatch tests — base64 encoding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_store_blob_decodes_base64_param() {
    let svc = BlobApiService(Arc::new(InMemoryBlobs::new()));

    // Send base64-encoded data and media_type
    let b64_data = simply_rpc::encode_base64(b"hello world");
    let params = json!({"data": b64_data, "media_type": "text/plain"});

    let dr = svc.dispatch("blob.store_blob", params).await.unwrap();
    let id: BlobId = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert_eq!(id.0, "blob_0");
}

#[tokio::test]
async fn dispatch_get_blob_encodes_base64_return() {
    let impl_ = Arc::new(InMemoryBlobs::new());
    let svc = BlobApiService(impl_.clone());

    // Store directly
    impl_.store_blob(b"binary data".to_vec(), "application/octet-stream").await.unwrap();

    // Get via dispatch — response should be base64 string
    let dr = svc.dispatch("blob.get_blob", json!("blob_0")).await.unwrap();
    let b64: String = serde_json::from_value(dr.result.unwrap()).unwrap();
    let decoded = simply_rpc::decode_base64(&b64).unwrap();
    assert_eq!(decoded, b"binary data");
}

// ---------------------------------------------------------------------------
// Client round-trip tests — base64 transparent to caller
// ---------------------------------------------------------------------------

struct MockBlobClient {
    svc: BlobApiService<InMemoryBlobs>,
}

#[async_trait]
impl RpcClient for MockBlobClient {
    type Stream = ();

    async fn rpc_call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        match self.svc.dispatch(method, params).await {
            Some(dr) => dr.result,
            None => Err(anyhow::anyhow!("unknown method: {method}")),
        }
    }

    async fn register_stream(&self, _id: &str) -> Self::Stream {}
    async fn unregister_stream(&self, _id: &str) {}
}

impl_remote_blob_api!(MockBlobClient);

#[tokio::test]
async fn client_store_and_get_blob_round_trip() {
    let client = MockBlobClient {
        svc: BlobApiService(Arc::new(InMemoryBlobs::new())),
    };

    // Store binary data — client encodes as base64 transparently
    let original = b"some binary content \x00\x01\x02".to_vec();
    let id = client.store_blob(original.clone(), "application/octet-stream").await.unwrap();

    // Get it back — client decodes base64 transparently
    let retrieved = client.get_blob(&id.0).await.unwrap();
    assert_eq!(retrieved, original);
}

#[tokio::test]
async fn client_store_blob_empty_data() {
    let client = MockBlobClient {
        svc: BlobApiService(Arc::new(InMemoryBlobs::new())),
    };

    let id = client.store_blob(vec![], "text/plain").await.unwrap();
    let retrieved = client.get_blob(&id.0).await.unwrap();
    assert!(retrieved.is_empty());
}

#[tokio::test]
async fn client_get_blob_not_found() {
    let client = MockBlobClient {
        svc: BlobApiService(Arc::new(InMemoryBlobs::new())),
    };

    let result = client.get_blob("nonexistent").await;
    assert!(result.is_err());
}
