//! Tests for binary data handling — BinaryResponse and base64_param.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use simply_rpc::{BinaryResponse, HttpMethod, RestDispatcher, RpcClient, RpcService};

// ---------------------------------------------------------------------------
// Test trait with binary methods
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
    async fn store_blob(&self, data: Vec<u8>, mime_type: &str) -> anyhow::Result<BlobId>;

    /// Get binary data as a BinaryResponse.
    #[rpc(get = "/blob/{id}", immutable_cache)]
    async fn get_blob(&self, id: &str) -> anyhow::Result<BinaryResponse>;
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
    async fn store_blob(&self, data: Vec<u8>, mime_type: &str) -> anyhow::Result<BlobId> {
        let id = BlobId(format!("blob_{}", self.blobs.lock().await.len()));
        self.blobs.lock().await.push((id.clone(), mime_type.to_string(), data));
        Ok(id)
    }

    async fn get_blob(&self, id: &str) -> anyhow::Result<BinaryResponse> {
        let blobs = self.blobs.lock().await;
        let (_, mime_type, data) = blobs
            .iter()
            .find(|(bid, _, _)| bid.0 == id)
            .ok_or_else(|| anyhow::anyhow!("not found: {id}"))?;
        Ok(BinaryResponse::new(data.clone(), mime_type.clone()))
    }
}

fn make_rd() -> (RestDispatcher, Arc<InMemoryBlobs>) {
    let impl_ = Arc::new(InMemoryBlobs::new());
    let svc = <dyn BlobApi>::service(impl_.clone());
    (RestDispatcher::new().register(svc), impl_)
}

// ---------------------------------------------------------------------------
// REST dispatch tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_store_blob_decodes_base64_param() {
    let (rd, _impl) = make_rd();

    let b64_data = simply_rpc::encode_base64(b"hello world");
    let params = serde_json::json!({"data": b64_data, "mime_type": "text/plain"});

    let result = rd.dispatch(HttpMethod::Post, "/blob", params).await.map(|rr| rr.result);
    let id: BlobId = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(id.0, "blob_0");
}

#[tokio::test]
async fn dispatch_get_blob_returns_binary_response() {
    let (rd, impl_) = make_rd();

    impl_.store_blob(b"binary data".to_vec(), "application/octet-stream").await.unwrap();

    let rr = rd.dispatch(HttpMethod::Get, "/blob/blob_0", Value::Null).await.unwrap();
    // Metadata should indicate binary_response and immutable_cache
    assert!(rr.meta.binary_response, "should be binary_response");
    assert!(rr.meta.immutable_cache, "should be immutable_cache");

    let br: BinaryResponse = serde_json::from_value(rr.result.unwrap()).unwrap();
    assert_eq!(br.data, b"binary data");
    assert_eq!(br.mime_type, "application/octet-stream");
}

// ---------------------------------------------------------------------------
// Client round-trip tests — binary data transparent to caller
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
            Some(rr) => rr.result,
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

    let br = client.get_blob(&id.0).await.unwrap();
    assert_eq!(br.data, original);
    assert_eq!(br.mime_type, "application/octet-stream");
}

#[tokio::test]
async fn client_store_blob_empty_data() {
    let client = MockBlobClient::new();

    let id = client.store_blob(vec![], "text/plain").await.unwrap();
    let br = client.get_blob(&id.0).await.unwrap();
    assert!(br.data.is_empty());
    assert_eq!(br.mime_type, "text/plain");
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
fn route_meta_for_blob_methods() {
    let meta = &BLOB_API_META;
    assert_eq!(meta.routes.len(), 2);

    let store = meta.routes.iter().find(|m| m.method_name == "blob.store_blob").unwrap();
    assert_eq!(store.http_method(), Some(HttpMethod::Post));
    assert_eq!(store.path_template, "/blob");
    assert!(!store.binary_response);
    assert!(!store.immutable_cache);

    let get = meta.routes.iter().find(|m| m.method_name == "blob.get_blob").unwrap();
    assert_eq!(get.http_method(), Some(HttpMethod::Get));
    assert_eq!(get.path_template, "/blob/{id}");
    assert!(get.binary_response, "get_blob returns BinaryResponse");
    assert!(get.immutable_cache, "get_blob has immutable_cache annotation");
}

// ---------------------------------------------------------------------------
// Raw HTTP integration tests — reqwest against a real axum server
// ---------------------------------------------------------------------------

async fn start_blob_server() -> (String, Arc<InMemoryBlobs>) {
    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{Method, StatusCode};
    use axum::response::{IntoResponse, Json as AxumJson};
    use axum::Router;

    let impl_ = Arc::new(InMemoryBlobs::new());
    let svc = <dyn BlobApi>::service(impl_.clone());
    let dispatcher = Arc::new(RestDispatcher::new().register(svc));

    #[derive(Clone)]
    struct AppState { rd: Arc<RestDispatcher> }

    async fn handler(State(state): State<AppState>, req: axum::extract::Request) -> axum::response::Response {
        let method = req.method().clone();
        let path = req.uri().path().to_string();

        let http_method = match method {
            Method::GET => HttpMethod::Get,
            Method::POST => HttpMethod::Post,
            _ => return (StatusCode::METHOD_NOT_ALLOWED, "").into_response(),
        };

        let body = match http_method {
            HttpMethod::Post => {
                let bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024).await.unwrap_or_default();
                if bytes.is_empty() { serde_json::Value::Null }
                else { serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null) }
            }
            _ => serde_json::Value::Null,
        };

        match state.rd.dispatch(http_method, &path, body).await {
            Some(rr) => {
                let value = match rr.result {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = e.to_string();
                        let status = if msg.contains("not found") { StatusCode::NOT_FOUND } else { StatusCode::INTERNAL_SERVER_ERROR };
                        return (status, msg).into_response();
                    }
                };
                if rr.meta.binary_response {
                    let br: BinaryResponse = match serde_json::from_value(value) {
                        Ok(br) => br,
                        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
                    };
                    let mut resp = axum::response::Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", &br.mime_type)
                        .header("Content-Length", br.data.len().to_string())
                        .body(Body::from(br.data))
                        .unwrap();
                    if rr.meta.immutable_cache {
                        let etag = path.rsplit('/').next().unwrap_or("");
                        resp.headers_mut().insert("Cache-Control", "public, max-age=31536000, immutable".parse().unwrap());
                        resp.headers_mut().insert("ETag", format!("\"{etag}\"").parse().unwrap());
                    }
                    resp
                } else {
                    AxumJson(value).into_response()
                }
            }
            None => (StatusCode::NOT_FOUND, "not found").into_response(),
        }
    }

    let app = Router::new()
        .fallback(handler)
        .with_state(AppState { rd: dispatcher });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move { axum::serve(listener, app).await.ok(); });

    (format!("http://127.0.0.1:{port}"), impl_)
}

#[tokio::test]
async fn http_store_blob() {
    let (base, _impl) = start_blob_server().await;

    let b64_data = simply_rpc::encode_base64(b"hello http");
    let resp = reqwest::Client::new()
        .post(format!("{base}/blob"))
        .json(&serde_json::json!({"data": b64_data, "mime_type": "text/plain"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let id: BlobId = resp.json().await.unwrap();
    assert!(id.0.starts_with("blob_"));
}

#[tokio::test]
async fn http_get_blob_returns_raw_bytes() {
    let (base, impl_) = start_blob_server().await;

    // Store directly
    impl_.store_blob(b"raw binary".to_vec(), "application/octet-stream").await.unwrap();

    // GET should return raw bytes, not JSON
    let resp = reqwest::get(format!("{base}/blob/blob_0")).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("Content-Type").unwrap(), "application/octet-stream");
    assert_eq!(resp.headers().get("Cache-Control").unwrap(), "public, max-age=31536000, immutable");
    assert!(resp.headers().get("ETag").is_some());

    let body = resp.bytes().await.unwrap();
    assert_eq!(&body[..], b"raw binary");
}

#[tokio::test]
async fn http_get_blob_not_found() {
    let (base, _impl) = start_blob_server().await;

    let resp = reqwest::get(format!("{base}/blob/missing")).await.unwrap();
    assert_ne!(resp.status(), 200);
}

#[tokio::test]
async fn http_store_and_get_round_trip() {
    let (base, _impl) = start_blob_server().await;
    let client = reqwest::Client::new();

    // Store via HTTP
    let original = b"round trip data \x00\xff".to_vec();
    let b64 = simply_rpc::encode_base64(&original);
    let resp = client.post(format!("{base}/blob"))
        .json(&serde_json::json!({"data": b64, "mime_type": "image/png"}))
        .send().await.unwrap();
    let id: BlobId = resp.json().await.unwrap();

    // Get via HTTP — raw bytes back
    let resp = reqwest::get(format!("{base}/blob/{}", id.0)).await.unwrap();
    assert_eq!(resp.headers().get("Content-Type").unwrap(), "image/png");
    let body = resp.bytes().await.unwrap();
    assert_eq!(&body[..], &original[..]);
}
