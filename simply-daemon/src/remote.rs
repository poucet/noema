//! RemoteDaemon — implements `DaemonApi` over REST + WebSocket.
//!
//! REST for request/response methods, WebSocket for streaming.

use std::sync::Arc;

use async_trait::async_trait;
use simply_rpc::{HttpMethod, RpcClient};
use simply_rpc::ws_client::ConnectionState;
use tokio::sync::{broadcast, watch};

use crate::api::*;
use crate::net::client::{DaemonWsConnection, daemon_demux};

/// A daemon client that talks to a remote daemon over REST + WebSocket.
pub struct RemoteDaemon {
    conn: DaemonWsConnection,
    http: reqwest::Client,
    base_url: String,
}

impl RemoteDaemon {
    pub async fn connect(addr: &str) -> anyhow::Result<Arc<Self>> {
        Self::connect_as(addr, "unknown").await
    }

    pub async fn connect_as(addr: &str, name: &str) -> anyhow::Result<Arc<Self>> {
        let conn = DaemonWsConnection::connect(addr, daemon_demux()).await?;
        let _ = conn.rpc_call("client.identify", serde_json::json!({ "name": name })).await;

        let base_url = format!("http://{addr}");

        Ok(Arc::new(Self {
            conn,
            http: reqwest::Client::new(),
            base_url,
        }))
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.conn.connection_state()
    }

    pub fn watch_connection_state(&self) -> watch::Receiver<ConnectionState> {
        self.conn.watch_state()
    }
}

#[async_trait]
impl RpcClient for RemoteDaemon {
    type Stream = broadcast::Receiver<DaemonEvent>;

    async fn rpc_call(&self, method: &str, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        self.conn.rpc_call(method, params).await
    }

    async fn register_stream(&self, id: &str) -> Self::Stream {
        self.conn.register_stream(id).await
    }

    async fn unregister_stream(&self, id: &str) {
        self.conn.unregister_stream(id).await
    }

    async fn register_bidi_stream<T, U>(
        &self,
        method: &str,
        _path: &str,
    ) -> anyhow::Result<simply_rpc::StreamHandle<T, U>>
    where
        T: serde::Serialize + Send + 'static,
        U: serde::de::DeserializeOwned + Send + 'static,
    {
        let event_method = format!("{method}.event");
        let input_method = format!("{method}.input");

        // Subscribe to events from the server
        let mut raw_rx = self.conn.register_raw_sink(&event_method).await;

        // Get a write channel for sending notifications
        let write_tx = self.conn.write_tx().await
            .ok_or_else(|| anyhow::anyhow!("disconnected"))?;

        // Create channels for the StreamHandle
        let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<T>(64);
        let (output_tx, output_rx) = tokio::sync::mpsc::channel::<U>(64);

        // Forward input: T → serialize → send as WS notification
        tokio::spawn(async move {
            while let Some(msg) = input_rx.recv().await {
                let notif = simply_rpc::protocol::WsNotification {
                    method: input_method.clone(),
                    params: serde_json::to_value(&msg).unwrap_or_default(),
                };
                let text = serde_json::to_string(&notif).unwrap_or_default();
                if write_tx.send(text).await.is_err() { break; }
            }
        });

        // Forward events: raw JSON → deserialize as U → output_tx
        tokio::spawn(async move {
            while let Some(value) = raw_rx.recv().await {
                match serde_json::from_value::<U>(value) {
                    Ok(event) => {
                        if output_tx.send(event).await.is_err() { break; }
                    }
                    Err(e) => {
                        tracing::warn!("bidi stream deserialize error: {e}");
                    }
                }
            }
        });

        Ok(simply_rpc::StreamHandle::new(input_tx, output_rx))
    }

    async fn rest_call(
        &self,
        http_method: HttpMethod,
        path: &str,
        body: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!("{}{}", self.base_url, path);

        let resp = match http_method {
            HttpMethod::Get => self.http.get(&url).send().await?,
            HttpMethod::Post => self.http.post(&url).json(&body).send().await?,
            HttpMethod::Put => self.http.put(&url).json(&body).send().await?,
            HttpMethod::Delete => self.http.delete(&url).send().await?,
        };

        let status = resp.status();
        if status.is_success() {
            let text = resp.text().await?;
            if text.is_empty() {
                Ok(serde_json::Value::Null)
            } else {
                Ok(serde_json::from_str(&text)?)
            }
        } else {
            let msg = resp.text().await.unwrap_or_else(|_| status.to_string());
            Err(anyhow::anyhow!("REST error {}: {}", status, msg))
        }
    }
}

impl_remote_session_api!(RemoteDaemon);
impl_remote_conversation_api!(RemoteDaemon);
impl_remote_asset_api!(RemoteDaemon);
impl_remote_mcp_api!(RemoteDaemon);
impl_remote_o_auth_api!(RemoteDaemon);
impl_remote_model_api!(RemoteDaemon);
impl_remote_voice_api!(RemoteDaemon);
impl_remote_core_api!(RemoteDaemon);

impl Daemon for RemoteDaemon {
    fn session(&self) -> &dyn SessionApi { self }
    fn conversation(&self) -> &dyn ConversationApi { self }
    fn mcp(&self) -> &dyn McpApi { self }
    fn oauth(&self) -> &dyn OAuthApi { self }
    fn model(&self) -> &dyn ModelApi { self }
    fn asset(&self) -> &dyn AssetApi { self }
    fn voice(&self) -> &dyn VoiceApi { self }
    fn core(&self) -> &dyn CoreApi { self }
    fn tools(&self) -> &dyn simply_core::ToolService { self }
}

/// Remote tool service — delegates to the daemon's `/mcp/tools` endpoints.
#[async_trait]
impl simply_core::ToolService for RemoteDaemon {
    async fn get_definitions(&self) -> Vec<llm::ToolDefinition> {
        McpApi::list_all_tools(self).await
            .unwrap_or_default()
            .into_iter()
            .map(|t| llm::ToolDefinition {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()),
                input_schema: serde_json::from_value(
                    serde_json::to_value(&*t.input_schema).unwrap_or_default()
                ).unwrap_or_default(),
            })
            .collect()
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<Vec<llm::ToolResultContent>> {
        let result = McpApi::call_tool_direct(
            self,
            CallToolRequestParam::new(name.to_string())
                .with_arguments(arguments.as_object().cloned().unwrap_or_default()),
        ).await?;
        Ok(result.content.into_iter().filter_map(|c| {
            match c.raw {
                rmcp::model::RawContent::Text(t) => Some(llm::ToolResultContent::text(t.text)),
                _ => None,
            }
        }).collect())
    }
}
