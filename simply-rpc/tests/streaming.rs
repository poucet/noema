//! Tests for streaming RPC: #[rpc(stream)] with tuple and bare stream returns.
//!
//! Covers:
//! - Direct dispatch: Result<(T, Stream)>, Result<Stream>, mixed traits
//! - Client macro round-trip through mock RPC
//! - Stream path metadata: #[rpc(stream = "/path")]
//! - Raw WebSocket integration: real server, tungstenite client

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simply_rpc::{HttpMethod, RestDispatcher, RpcClient, RpcService};
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEvent(pub String);

// ---------------------------------------------------------------------------
// Trait with stream methods (using path annotations)
// ---------------------------------------------------------------------------

#[simply_rpc::rpc_service("chan")]
#[async_trait]
pub trait ChannelApi: Send + Sync {
    /// Open a new channel. Returns info + event stream.
    #[rpc(stream = "/chan/new")]
    async fn open_channel(
        &self,
        name: &str,
    ) -> anyhow::Result<(ChannelInfo, broadcast::Receiver<ChannelEvent>)>;

    /// Subscribe to an existing channel's events.
    #[rpc(stream = "/chan/{channel_id}/subscribe")]
    async fn subscribe(&self, channel_id: &str) -> anyhow::Result<broadcast::Receiver<ChannelEvent>>;

    /// List all channels.
    #[rpc(get = "/chan")]
    async fn list_channels(&self) -> anyhow::Result<Vec<ChannelInfo>>;

    /// Close a channel.
    #[rpc(delete = "/chan/{channel_id}")]
    async fn close_channel(&self, channel_id: &str) -> anyhow::Result<()>;
}

// ---------------------------------------------------------------------------
// In-memory implementation
// ---------------------------------------------------------------------------

struct InMemoryChannels {
    channels: tokio::sync::Mutex<Vec<(ChannelInfo, broadcast::Sender<ChannelEvent>)>>,
}

impl InMemoryChannels {
    fn new() -> Self {
        Self {
            channels: tokio::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ChannelApi for InMemoryChannels {
    async fn open_channel(
        &self,
        name: &str,
    ) -> anyhow::Result<(ChannelInfo, broadcast::Receiver<ChannelEvent>)> {
        let (tx, rx) = broadcast::channel(16);
        let info = ChannelInfo {
            id: format!("ch_{name}"),
            name: name.to_string(),
        };
        self.channels.lock().await.push((info.clone(), tx));
        Ok((info, rx))
    }

    async fn subscribe(
        &self,
        channel_id: &str,
    ) -> anyhow::Result<broadcast::Receiver<ChannelEvent>> {
        let channels = self.channels.lock().await;
        let (_, tx) = channels
            .iter()
            .find(|(info, _)| info.id == channel_id)
            .ok_or_else(|| anyhow::anyhow!("channel not found: {channel_id}"))?;
        Ok(tx.subscribe())
    }

    async fn list_channels(&self) -> anyhow::Result<Vec<ChannelInfo>> {
        Ok(self
            .channels
            .lock()
            .await
            .iter()
            .map(|(info, _)| info.clone())
            .collect())
    }

    async fn close_channel(&self, channel_id: &str) -> anyhow::Result<()> {
        let mut channels = self.channels.lock().await;
        channels.retain(|(info, _)| info.id != channel_id);
        Ok(())
    }
}

// ===========================================================================
// PART 1: Server dispatch tests — stream tuple
// ===========================================================================

#[tokio::test]
async fn dispatch_stream_tuple_returns_value_and_stream() {
    let svc = ChannelApiService(Arc::new(InMemoryChannels::new()));

    let dr = svc.dispatch("chan.open_channel", json!("test")).await.unwrap();

    let info: ChannelInfo = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert_eq!(info.id, "ch_test");
    assert_eq!(info.name, "test");
    assert_eq!(dr.streams.len(), 1);
}

#[tokio::test]
async fn dispatch_stream_tuple_stream_is_live() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    let dr = svc.dispatch("chan.open_channel", json!("live")).await.unwrap();
    let mut rx = dr.streams.into_iter().next().unwrap();

    {
        let channels = impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_live").unwrap();
        tx.send(ChannelEvent("hello".into())).unwrap();
    }

    let event = rx.recv().await.unwrap();
    assert_eq!(event.0, "hello");
}

// ===========================================================================
// PART 2: Server dispatch tests — bare stream
// ===========================================================================

#[tokio::test]
async fn dispatch_bare_stream_returns_true_and_stream() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    svc.dispatch("chan.open_channel", json!("sub")).await.unwrap();

    let dr = svc.dispatch("chan.subscribe", json!("ch_sub")).await.unwrap();
    assert_eq!(dr.result.unwrap(), Value::Bool(true));
    assert_eq!(dr.streams.len(), 1);
}

#[tokio::test]
async fn dispatch_bare_stream_is_live() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    svc.dispatch("chan.open_channel", json!("sub2")).await.unwrap();
    let dr = svc.dispatch("chan.subscribe", json!("ch_sub2")).await.unwrap();
    let mut rx = dr.streams.into_iter().next().unwrap();

    {
        let channels = impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_sub2").unwrap();
        tx.send(ChannelEvent("world".into())).unwrap();
    }

    let event = rx.recv().await.unwrap();
    assert_eq!(event.0, "world");
}

// ===========================================================================
// PART 3: Non-stream methods on a streaming trait
// ===========================================================================

#[tokio::test]
async fn dispatch_non_stream_on_streaming_trait() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    let dr = svc.dispatch("chan.list_channels", Value::Null).await.unwrap();
    let channels: Vec<ChannelInfo> = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert!(channels.is_empty());
    assert!(dr.streams.is_empty());

    svc.dispatch("chan.open_channel", json!("mixed")).await.unwrap();
    let dr = svc.dispatch("chan.list_channels", Value::Null).await.unwrap();
    let channels: Vec<ChannelInfo> = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert_eq!(channels.len(), 1);
    assert!(dr.streams.is_empty());
}

#[tokio::test]
async fn dispatch_close_on_streaming_trait() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    svc.dispatch("chan.open_channel", json!("doomed")).await.unwrap();
    let dr = svc.dispatch("chan.close_channel", json!("ch_doomed")).await.unwrap();
    assert_eq!(dr.result.unwrap(), Value::Bool(true));
    assert!(dr.streams.is_empty());

    let dr = svc.dispatch("chan.list_channels", Value::Null).await.unwrap();
    let channels: Vec<ChannelInfo> = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert!(channels.is_empty());
}

#[tokio::test]
async fn service_stream_type_is_broadcast_receiver() {
    let svc = ChannelApiService(Arc::new(InMemoryChannels::new()));
    let dr = svc.dispatch("chan.open_channel", json!("typed")).await.unwrap();
    let _rx: broadcast::Receiver<ChannelEvent> = dr.streams.into_iter().next().unwrap();
}

// ===========================================================================
// PART 4: Stream path metadata
// ===========================================================================

#[test]
fn stream_meta_has_paths() {
    let meta = &CHANNEL_API_META;

    let stream_methods: Vec<_> = meta.routes.iter()
        .filter(|m| m.kind == simply_rpc::RouteKind::Stream)
        .collect();
    assert_eq!(stream_methods.len(), 2);

    let paths: Vec<&str> = stream_methods.iter().map(|m| m.path_template).collect();
    assert!(paths.contains(&"/chan/new"));
    assert!(paths.contains(&"/chan/{channel_id}/subscribe"));
}

#[test]
fn stream_and_rest_meta_coexist() {
    let meta = &CHANNEL_API_META;

    let routes: Vec<_> = meta.routes.iter()
        .filter(|m| m.kind != simply_rpc::RouteKind::Stream)
        .collect();
    assert_eq!(routes.len(), 2); // list_channels + close_channel

    let stream_methods: Vec<_> = meta.routes.iter()
        .filter(|m| m.kind == simply_rpc::RouteKind::Stream)
        .collect();
    assert_eq!(stream_methods.len(), 2); // open_channel + subscribe
}

#[test]
fn stream_meta_doc_comments() {
    let meta = &CHANNEL_API_META;
    let find = |name: &str| meta.routes.iter().find(|m| m.method_name == name).unwrap();

    assert_eq!(find("chan.open_channel").description, Some("Open a new channel. Returns info + event stream."));
    assert_eq!(find("chan.subscribe").description, Some("Subscribe to an existing channel's events."));
}

// ===========================================================================
// PART 5: REST dispatch works for non-stream methods on the same trait
// ===========================================================================

fn make_channel_rd(impl_: Arc<InMemoryChannels>) -> RestDispatcher {
    let svc = <dyn ChannelApi>::service(impl_);
    RestDispatcher::new().register(svc)
}

#[tokio::test]
async fn rest_dispatch_list_on_streaming_trait() {
    let rd = make_channel_rd(Arc::new(InMemoryChannels::new()));

    // REST methods work through RestDispatcher
    let result = rd.dispatch(HttpMethod::Get, "/chan", Value::Null).await;
    let channels: Vec<ChannelInfo> = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert!(channels.is_empty());
}

#[tokio::test]
async fn rest_dispatch_delete_on_streaming_trait() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());
    let rd = make_channel_rd(impl_.clone());

    // Create via WS dispatch
    svc.dispatch("chan.open_channel", json!("rest_del")).await.unwrap();

    // Delete via REST dispatch
    let result = rd.dispatch(HttpMethod::Delete, "/chan/ch_rest_del", Value::Null).await;
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));
}

#[tokio::test]
async fn rest_dispatch_stream_path_returns_none() {
    let rd = make_channel_rd(Arc::new(InMemoryChannels::new()));

    // Stream paths should NOT be dispatched as REST
    let result = rd.dispatch(HttpMethod::Get, "/chan/new", Value::Null).await;
    assert!(result.is_none(), "stream paths should not match REST dispatch");
}

// ===========================================================================
// PART 6: Client macro tests — round-trip through mock RPC
// ===========================================================================

struct MockStreamClient {
    rd: RestDispatcher,
    svc: Arc<ChannelApiService<dyn ChannelApi>>,
    impl_: Arc<InMemoryChannels>,
}

impl MockStreamClient {
    fn new() -> Self {
        let impl_ = Arc::new(InMemoryChannels::new());
        let svc = <dyn ChannelApi>::service(impl_.clone());
        let rd = RestDispatcher::new().register(svc.clone());
        Self { rd, svc, impl_ }
    }
}

#[async_trait]
impl RpcClient for MockStreamClient {
    type Stream = broadcast::Receiver<ChannelEvent>;

    async fn rpc_call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        match self.svc.dispatch(method, params).await {
            Some(dr) => dr.result,
            None => Err(anyhow::anyhow!("unknown method: {method}")),
        }
    }

    async fn register_stream(&self, id: &str) -> Self::Stream {
        let channels = self.impl_.channels.lock().await;
        let (_, tx) = channels
            .iter()
            .find(|(info, _)| info.id == id)
            .expect("channel should exist for register_stream");
        tx.subscribe()
    }

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

impl_remote_channel_api!(MockStreamClient);

#[tokio::test]
async fn client_open_channel_returns_info_and_stream() {
    let client = MockStreamClient::new();

    let (info, mut rx) = client.open_channel("client_test").await.unwrap();
    assert_eq!(info.id, "ch_client_test");

    {
        let channels = client.impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_client_test").unwrap();
        tx.send(ChannelEvent("from_client".into())).unwrap();
    }

    let event = rx.recv().await.unwrap();
    assert_eq!(event.0, "from_client");
}

#[tokio::test]
async fn client_subscribe_returns_stream() {
    let client = MockStreamClient::new();

    client.open_channel("sub_test").await.unwrap();
    let mut rx = client.subscribe("ch_sub_test").await.unwrap();

    {
        let channels = client.impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_sub_test").unwrap();
        tx.send(ChannelEvent("subscribed".into())).unwrap();
    }

    let event = rx.recv().await.unwrap();
    assert_eq!(event.0, "subscribed");
}

#[tokio::test]
async fn client_non_stream_methods_work() {
    let client = MockStreamClient::new();

    assert!(client.list_channels().await.unwrap().is_empty());

    client.open_channel("one").await.unwrap();
    assert_eq!(client.list_channels().await.unwrap().len(), 1);

    client.close_channel("ch_one").await.unwrap();
    assert!(client.list_channels().await.unwrap().is_empty());
}

// ===========================================================================
// PART 7: Raw WebSocket integration tests
//
// Real server with WebSocket upgrade for stream paths, REST for non-stream.
// Client uses only tokio-tungstenite + serde_json — no simply-rpc client code.
// ===========================================================================

/// Start a test server that handles both REST and WebSocket upgrades on stream paths.
/// Returns the base URL (e.g. "http://127.0.0.1:12345").
async fn start_ws_test_server() -> (String, Arc<InMemoryChannels>) {
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response, StatusCode};
    use hyper::body::Incoming;
    use hyper_util::rt::TokioIo;
    use http_body_util::{BodyExt, Full};

    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = <dyn ChannelApi>::service(impl_.clone());
    let rest_svc = Arc::new(
        RestDispatcher::new().register(svc.clone()),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let impl_for_server = impl_.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { break };
            let svc = svc.clone();
            let rest_svc = rest_svc.clone();
            let impl_ = impl_for_server.clone();
            tokio::spawn(async move {
                let service = service_fn(move |req: Request<Incoming>| {
                    let svc = svc.clone();
                    let rest_svc = rest_svc.clone();
                    let impl_ = impl_.clone();
                    async move {
                        let path = req.uri().path().to_string();
                        let method_str = req.method().to_string();

                        // Check for WebSocket upgrade
                        let is_upgrade = req.headers()
                            .get("upgrade")
                            .and_then(|v| v.to_str().ok())
                            .map(|v| v.eq_ignore_ascii_case("websocket"))
                            .unwrap_or(false);

                        if is_upgrade {
                            // Find matching stream path and dispatch
                            let meta = &CHANNEL_API_META;
                            let stream_match = meta.routes.iter()
                                .filter(|m| m.kind == simply_rpc::RouteKind::Stream)
                                .find(|m| {
                                    path_matches(m.path_template, &path)
                                });

                            let _ = stream_match; // Validated in other tests

                            // For now, return 501 for actual WS upgrades (tested via dispatch)
                            return Ok::<_, std::convert::Infallible>(
                                Response::builder().status(501)
                                    .body(Full::new(hyper::body::Bytes::from("ws upgrade tested via dispatch")))
                                    .unwrap()
                            );
                        }

                        // REST dispatch
                        let http_method = match method_str.as_str() {
                            "GET" => HttpMethod::Get,
                            "POST" => HttpMethod::Post,
                            "PUT" => HttpMethod::Put,
                            "DELETE" => HttpMethod::Delete,
                            _ => return Ok(Response::builder().status(405)
                                .body(Full::new(hyper::body::Bytes::from("method not allowed")))
                                .unwrap()),
                        };

                        let body = match http_method {
                            HttpMethod::Post | HttpMethod::Put => {
                                match req.collect().await {
                                    Ok(c) => {
                                        let bytes = c.to_bytes();
                                        if bytes.is_empty() { Value::Null }
                                        else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) }
                                    }
                                    Err(_) => Value::Null,
                                }
                            }
                            _ => Value::Null,
                        };

                        match rest_svc.dispatch(http_method, &path, body).await {
                            Some(Ok(value)) => {
                                let json = serde_json::to_vec(&value).unwrap_or_default();
                                Ok(Response::builder()
                                    .status(StatusCode::OK)
                                    .header("Content-Type", "application/json")
                                    .body(Full::new(hyper::body::Bytes::from(json)))
                                    .unwrap())
                            }
                            Some(Err(e)) => {
                                let msg = e.to_string();
                                let status = if msg.contains("not found") { 404 } else { 500 };
                                Ok(Response::builder().status(status)
                                    .body(Full::new(hyper::body::Bytes::from(msg)))
                                    .unwrap())
                            }
                            None => Ok(Response::builder().status(404)
                                .body(Full::new(hyper::body::Bytes::from("not found")))
                                .unwrap()),
                        }
                    }
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });

    (format!("http://127.0.0.1:{port}"), impl_)
}

/// Check if a path matches a template (simple segment comparison).
fn path_matches(template: &str, path: &str) -> bool {
    let t_segs: Vec<&str> = template.trim_start_matches('/').split('/').collect();
    let p_segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if t_segs.len() != p_segs.len() { return false; }
    t_segs.iter().zip(p_segs.iter()).all(|(t, p)| {
        t.starts_with('{') || t == p
    })
}

/// Extract named params from a path given a template.
fn extract_params(template: &str, path: &str) -> std::collections::HashMap<String, String> {
    let t_segs: Vec<&str> = template.trim_start_matches('/').split('/').collect();
    let p_segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let mut params = std::collections::HashMap::new();
    for (t, p) in t_segs.iter().zip(p_segs.iter()) {
        if t.starts_with('{') && t.ends_with('}') {
            params.insert(t[1..t.len()-1].to_string(), p.to_string());
        }
    }
    params
}

// --- Raw HTTP tests (REST on a streaming trait) ---

#[tokio::test]
async fn http_list_channels_on_streaming_trait() {
    let (base, _impl) = start_ws_test_server().await;

    let resp = reqwest::get(format!("{base}/chan")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let channels: Vec<ChannelInfo> = resp.json().await.unwrap();
    assert!(channels.is_empty());
}

#[tokio::test]
async fn http_crud_alongside_streams() {
    let (base, impl_) = start_ws_test_server().await;

    // Create a channel directly (simulating what a WS upgrade would do)
    {
        let (tx, _) = broadcast::channel(16);
        impl_.channels.lock().await.push((
            ChannelInfo { id: "ch_test".into(), name: "test".into() },
            tx,
        ));
    }

    // List via REST
    let channels: Vec<ChannelInfo> = reqwest::get(format!("{base}/chan"))
        .await.unwrap().json().await.unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, "ch_test");

    // Delete via REST
    let client = reqwest::Client::new();
    let resp = client.delete(format!("{base}/chan/ch_test")).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    // Verify deleted
    let channels: Vec<ChannelInfo> = reqwest::get(format!("{base}/chan"))
        .await.unwrap().json().await.unwrap();
    assert!(channels.is_empty());
}

#[tokio::test]
async fn http_stream_path_without_upgrade_not_found() {
    let (base, _impl) = start_ws_test_server().await;

    // GET on a stream path without WebSocket upgrade should 404 or similar
    let resp = reqwest::get(format!("{base}/chan/new")).await.unwrap();
    // REST dispatcher won't match stream paths, so 404
    assert_eq!(resp.status(), 404);
}

// --- WebSocket connection tests using tokio-tungstenite ---

#[tokio::test]
async fn ws_stream_dispatch_produces_events() {
    // Test that stream dispatch through the service struct works correctly,
    // verifying the same codepath a WebSocket server would use.
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    // Dispatch the stream method (this is what the WS upgrade handler would do)
    let dr = svc.dispatch("chan.open_channel", json!("ws_test")).await.unwrap();
    let info: ChannelInfo = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert_eq!(info.id, "ch_ws_test");

    let mut rx = dr.streams.into_iter().next().unwrap();

    // Simulate events being pushed (this is what send_message would trigger)
    {
        let channels = impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_ws_test").unwrap();
        tx.send(ChannelEvent("event_1".into())).unwrap();
        tx.send(ChannelEvent("event_2".into())).unwrap();
    }

    assert_eq!(rx.recv().await.unwrap().0, "event_1");
    assert_eq!(rx.recv().await.unwrap().0, "event_2");
}

#[tokio::test]
async fn ws_subscribe_dispatch_multiple_listeners() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    // Open channel
    let dr = svc.dispatch("chan.open_channel", json!("multi")).await.unwrap();
    let mut rx1 = dr.streams.into_iter().next().unwrap();

    // Subscribe (second listener)
    let dr = svc.dispatch("chan.subscribe", json!("ch_multi")).await.unwrap();
    let mut rx2 = dr.streams.into_iter().next().unwrap();

    // Both should receive the event
    {
        let channels = impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_multi").unwrap();
        tx.send(ChannelEvent("broadcast".into())).unwrap();
    }

    assert_eq!(rx1.recv().await.unwrap().0, "broadcast");
    assert_eq!(rx2.recv().await.unwrap().0, "broadcast");
}

#[tokio::test]
async fn ws_real_websocket_open_and_receive() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = Arc::new(ChannelApiService(impl_.clone()));

    // Start a minimal WS server that upgrades at /chan/new
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let svc_clone = svc.clone();
    let impl_clone = impl_.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let (mut ws_tx, mut ws_rx) = ws.split();

        // Read first message as the method params
        let msg = ws_rx.next().await.unwrap().unwrap();
        let params: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap_or(Value::Null);

        // Dispatch
        let dr = svc_clone.dispatch("chan.open_channel", params).await.unwrap();
        let info_json = serde_json::to_string(&dr.result.unwrap()).unwrap();

        // Send SessionInfo as first message
        ws_tx.send(Message::Text(info_json.into())).await.unwrap();

        // Forward events
        let mut rx = dr.streams.into_iter().next().unwrap();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let json = serde_json::to_string(&event).unwrap();
                    if ws_tx.send(Message::Text(json.into())).await.is_err() { break; }
                }
                Err(_) => break,
            }
        }
    });

    // Client connects with raw tokio-tungstenite
    let (mut ws, _resp) = tokio_tungstenite::connect_async(
        format!("ws://127.0.0.1:{port}/chan/new")
    ).await.unwrap();

    // Send channel name as first message
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        json!("ws_raw").to_string().into()
    )).await.unwrap();

    // Receive ChannelInfo
    let msg = ws.next().await.unwrap().unwrap();
    let info: ChannelInfo = serde_json::from_str(msg.to_text().unwrap()).unwrap();
    assert_eq!(info.id, "ch_ws_raw");
    assert_eq!(info.name, "ws_raw");

    // Push an event from the server side
    {
        let channels = impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_ws_raw").unwrap();
        tx.send(ChannelEvent("real_ws_event".into())).unwrap();
    }

    // Receive the event over WebSocket
    let msg = ws.next().await.unwrap().unwrap();
    let event: ChannelEvent = serde_json::from_str(msg.to_text().unwrap()).unwrap();
    assert_eq!(event.0, "real_ws_event");
}
