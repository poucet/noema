//! Tests for streaming RPC: #[rpc(stream)] with tuple and bare stream returns.
//!
//! Covers: Result<(T, Stream)>, Result<Stream>, mixed traits with stream + non-stream methods.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simply_rpc::{RpcClient, RpcService};
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ChannelEvent(pub String);

// ---------------------------------------------------------------------------
// Trait with stream methods
// ---------------------------------------------------------------------------

#[simply_rpc::rpc_service("chan")]
#[async_trait]
pub trait ChannelApi: Send + Sync {
    /// Stream tuple: returns info + event receiver
    #[rpc(stream)]
    async fn open_channel(
        &self,
        name: &str,
    ) -> anyhow::Result<(ChannelInfo, broadcast::Receiver<ChannelEvent>)>;

    /// Bare stream: returns just the receiver
    #[rpc(stream)]
    async fn subscribe(&self, channel_id: &str) -> anyhow::Result<broadcast::Receiver<ChannelEvent>>;

    /// Non-stream method alongside stream methods
    async fn list_channels(&self) -> anyhow::Result<Vec<ChannelInfo>>;

    /// Result<()> alongside stream methods
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

// ---------------------------------------------------------------------------
// Server dispatch tests — stream tuple
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_stream_tuple_returns_value_and_stream() {
    let svc = ChannelApiService(Arc::new(InMemoryChannels::new()));

    let dr = svc.dispatch("chan.open_channel", json!("test")).await.unwrap();

    // Result is the serialized ChannelInfo (without the stream)
    let info: ChannelInfo = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert_eq!(info.id, "ch_test");
    assert_eq!(info.name, "test");

    // Stream is returned separately
    assert_eq!(dr.streams.len(), 1);
}

#[tokio::test]
async fn dispatch_stream_tuple_stream_is_live() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    let dr = svc.dispatch("chan.open_channel", json!("live")).await.unwrap();
    let mut rx = dr.streams.into_iter().next().unwrap();

    // Send an event through the channel
    {
        let channels = impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_live").unwrap();
        tx.send(ChannelEvent("hello".into())).unwrap();
    }

    let event = rx.recv().await.unwrap();
    assert_eq!(event.0, "hello");
}

// ---------------------------------------------------------------------------
// Server dispatch tests — bare stream
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_bare_stream_returns_true_and_stream() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    // First create a channel
    svc.dispatch("chan.open_channel", json!("sub")).await.unwrap();

    // Subscribe returns bare stream
    let dr = svc.dispatch("chan.subscribe", json!("ch_sub")).await.unwrap();
    assert_eq!(dr.result.unwrap(), Value::Bool(true));
    assert_eq!(dr.streams.len(), 1);
}

#[tokio::test]
async fn dispatch_bare_stream_is_live() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    // Create channel + subscribe
    svc.dispatch("chan.open_channel", json!("sub2")).await.unwrap();
    let dr = svc.dispatch("chan.subscribe", json!("ch_sub2")).await.unwrap();
    let mut rx = dr.streams.into_iter().next().unwrap();

    // Send event
    {
        let channels = impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_sub2").unwrap();
        tx.send(ChannelEvent("world".into())).unwrap();
    }

    let event = rx.recv().await.unwrap();
    assert_eq!(event.0, "world");
}

// ---------------------------------------------------------------------------
// Non-stream methods on a trait that also has stream methods
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_non_stream_on_streaming_trait() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let svc = ChannelApiService(impl_.clone());

    // list_channels — no streams
    let dr = svc.dispatch("chan.list_channels", Value::Null).await.unwrap();
    let channels: Vec<ChannelInfo> = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert!(channels.is_empty());
    assert!(dr.streams.is_empty());

    // Create a channel, then list
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

    // Verify closed
    let dr = svc.dispatch("chan.list_channels", Value::Null).await.unwrap();
    let channels: Vec<ChannelInfo> = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert!(channels.is_empty());
}

// ---------------------------------------------------------------------------
// Stream type is correctly set on the service
// ---------------------------------------------------------------------------

#[tokio::test]
async fn service_stream_type_is_broadcast_receiver() {
    let svc = ChannelApiService(Arc::new(InMemoryChannels::new()));

    // The service's Stream type should be broadcast::Receiver<ChannelEvent>
    // We verify this by calling a stream method and using the stream
    let dr = svc.dispatch("chan.open_channel", json!("typed")).await.unwrap();
    // If this compiles, the type is correct
    let _rx: broadcast::Receiver<ChannelEvent> = dr.streams.into_iter().next().unwrap();
}

// ---------------------------------------------------------------------------
// Client macro tests — stream methods via mock RPC
// ---------------------------------------------------------------------------

/// Mock client that dispatches locally, tracks stream registrations.
struct MockStreamClient {
    svc: ChannelApiService<InMemoryChannels>,
    impl_: Arc<InMemoryChannels>,
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
        // Simulate: subscribe to the channel by ID
        let channels = self.impl_.channels.lock().await;
        let (_, tx) = channels
            .iter()
            .find(|(info, _)| info.id == id)
            .expect("channel should exist for register_stream");
        tx.subscribe()
    }

    async fn unregister_stream(&self, _id: &str) {}
}

impl_remote_channel_api!(MockStreamClient);

#[tokio::test]
async fn client_open_channel_returns_info_and_stream() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let client = MockStreamClient {
        svc: ChannelApiService(impl_.clone()),
        impl_: impl_.clone(),
    };

    let (info, mut rx) = client.open_channel("client_test").await.unwrap();
    assert_eq!(info.id, "ch_client_test");

    // Stream should be live
    {
        let channels = impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_client_test").unwrap();
        tx.send(ChannelEvent("from_client".into())).unwrap();
    }

    let event = rx.recv().await.unwrap();
    assert_eq!(event.0, "from_client");
}

#[tokio::test]
async fn client_subscribe_returns_stream() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let client = MockStreamClient {
        svc: ChannelApiService(impl_.clone()),
        impl_: impl_.clone(),
    };

    // Create a channel first
    client.open_channel("sub_test").await.unwrap();

    // Subscribe
    let mut rx = client.subscribe("ch_sub_test").await.unwrap();

    // Verify stream works
    {
        let channels = impl_.channels.lock().await;
        let (_, tx) = channels.iter().find(|(i, _)| i.id == "ch_sub_test").unwrap();
        tx.send(ChannelEvent("subscribed".into())).unwrap();
    }

    let event = rx.recv().await.unwrap();
    assert_eq!(event.0, "subscribed");
}

#[tokio::test]
async fn client_non_stream_methods_work() {
    let impl_ = Arc::new(InMemoryChannels::new());
    let client = MockStreamClient {
        svc: ChannelApiService(impl_.clone()),
        impl_: impl_.clone(),
    };

    // list_channels
    let channels = client.list_channels().await.unwrap();
    assert!(channels.is_empty());

    // open + list
    client.open_channel("one").await.unwrap();
    let channels = client.list_channels().await.unwrap();
    assert_eq!(channels.len(), 1);

    // close + list
    client.close_channel("ch_one").await.unwrap();
    let channels = client.list_channels().await.unwrap();
    assert!(channels.is_empty());
}
