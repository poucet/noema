//! Tests for REST dispatch: metadata, direct dispatch, client macro round-trip,
//! RestDispatcher routing, and raw HTTP integration tests using reqwest.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simply_rpc::{HttpMethod, RestDispatcher, RestService, RpcClient, RpcService};

// ---------------------------------------------------------------------------
// Test types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Widget {
    pub id: String,
    pub label: String,
}

// ---------------------------------------------------------------------------
// Test trait with REST annotations
// ---------------------------------------------------------------------------

#[simply_rpc::rpc_service("widget")]
#[async_trait]
pub trait WidgetApi: Send + Sync {
    /// List all widgets
    #[rpc(get = "/widget")]
    async fn list_widgets(&self) -> anyhow::Result<Vec<Widget>>;

    /// Get a widget by ID
    #[rpc(get = "/widget/{id}")]
    async fn get_widget(&self, id: &str) -> anyhow::Result<Widget>;

    /// Create a new widget
    #[rpc(post = "/widget")]
    async fn create_widget(&self, widget: Widget) -> anyhow::Result<Widget>;

    /// Delete a widget
    #[rpc(delete = "/widget/{id}")]
    async fn delete_widget(&self, id: &str) -> anyhow::Result<()>;

    /// Rename a widget (path param + body param)
    #[rpc(put = "/widget/{id}")]
    async fn rename_widget(&self, id: &str, label: &str) -> anyhow::Result<()>;

    /// Nested path: send a command to a widget
    #[rpc(post = "/widget/{id}/command")]
    async fn send_command(&self, id: &str, action: &str) -> anyhow::Result<String>;

    /// Kill the service (excluded from tools)
    #[rpc(post = "/widget/kill", no_tool)]
    async fn kill(&self) -> anyhow::Result<()>;

    /// Stream method (no REST)
    #[rpc(stream)]
    async fn watch_widgets(&self) -> anyhow::Result<tokio::sync::broadcast::Receiver<String>>;

    /// Skipped method
    #[rpc(skip)]
    async fn internal_op(&self) -> anyhow::Result<()>;

    /// Raw return, no REST annotation
    async fn widget_count(&self) -> usize;
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

struct InMemoryWidgets {
    widgets: tokio::sync::Mutex<Vec<Widget>>,
    events: tokio::sync::broadcast::Sender<String>,
}

impl InMemoryWidgets {
    fn new(widgets: Vec<Widget>) -> Self {
        let (events, _) = tokio::sync::broadcast::channel(16);
        Self {
            widgets: tokio::sync::Mutex::new(widgets),
            events,
        }
    }
}

#[async_trait]
impl WidgetApi for InMemoryWidgets {
    async fn list_widgets(&self) -> anyhow::Result<Vec<Widget>> {
        Ok(self.widgets.lock().await.clone())
    }

    async fn get_widget(&self, id: &str) -> anyhow::Result<Widget> {
        self.widgets.lock().await.iter()
            .find(|w| w.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("not found: {id}"))
    }

    async fn create_widget(&self, widget: Widget) -> anyhow::Result<Widget> {
        self.widgets.lock().await.push(widget.clone());
        Ok(widget)
    }

    async fn delete_widget(&self, id: &str) -> anyhow::Result<()> {
        let mut widgets = self.widgets.lock().await;
        let len_before = widgets.len();
        widgets.retain(|w| w.id != id);
        if widgets.len() == len_before {
            anyhow::bail!("not found: {id}");
        }
        Ok(())
    }

    async fn rename_widget(&self, id: &str, label: &str) -> anyhow::Result<()> {
        let mut widgets = self.widgets.lock().await;
        let w = widgets.iter_mut().find(|w| w.id == id)
            .ok_or_else(|| anyhow::anyhow!("not found: {id}"))?;
        w.label = label.to_string();
        Ok(())
    }

    async fn send_command(&self, id: &str, action: &str) -> anyhow::Result<String> {
        Ok(format!("{id}:{action}"))
    }

    async fn kill(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn watch_widgets(&self) -> anyhow::Result<tokio::sync::broadcast::Receiver<String>> {
        Ok(self.events.subscribe())
    }

    async fn internal_op(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn widget_count(&self) -> usize {
        self.widgets.lock().await.len()
    }
}

fn make_svc(widgets: Vec<Widget>) -> WidgetApiService<InMemoryWidgets> {
    WidgetApiService(Arc::new(InMemoryWidgets::new(widgets)))
}

fn make_rd_from_widgets(widgets: Vec<Widget>) -> RestDispatcher {
    let impl_ = Arc::new(InMemoryWidgets::new(widgets));
    let svc = <dyn WidgetApi>::service(impl_);
    RestDispatcher::new().register(svc)
}

// ===========================================================================
// PART 1: Metadata tests
// ===========================================================================

#[test]
fn rest_meta_generated() {
    let meta = &WIDGET_API_META;
    assert_eq!(meta.prefix, "widget");
    assert!(!meta.routes.is_empty());

    let rest_names: Vec<&str> = meta.routes.iter().map(|m| m.method_name).collect();
    assert!(rest_names.contains(&"widget.list_widgets"));
    assert!(rest_names.contains(&"widget.get_widget"));
    assert!(rest_names.contains(&"widget.create_widget"));
    assert!(rest_names.contains(&"widget.delete_widget"));
    assert!(rest_names.contains(&"widget.rename_widget"));
    assert!(rest_names.contains(&"widget.send_command"));
    assert!(rest_names.contains(&"widget.kill"));

    // Stream, skip, and unannotated methods should NOT have REST entries
    assert!(!rest_names.contains(&"widget.watch_widgets"));
    assert!(!rest_names.contains(&"widget.internal_op"));
    assert!(!rest_names.contains(&"widget.widget_count"));
}

#[test]
fn rest_meta_http_methods_correct() {
    let meta = &WIDGET_API_META;
    let find = |name: &str| meta.routes.iter().find(|m| m.method_name == name).unwrap();

    assert_eq!(find("widget.list_widgets").http_method(), Some(HttpMethod::Get));
    assert_eq!(find("widget.get_widget").http_method(), Some(HttpMethod::Get));
    assert_eq!(find("widget.create_widget").http_method(), Some(HttpMethod::Post));
    assert_eq!(find("widget.delete_widget").http_method(), Some(HttpMethod::Delete));
    assert_eq!(find("widget.rename_widget").http_method(), Some(HttpMethod::Put));
    assert_eq!(find("widget.send_command").http_method(), Some(HttpMethod::Post));
    assert_eq!(find("widget.kill").http_method(), Some(HttpMethod::Post));
}

#[test]
fn rest_meta_path_templates_correct() {
    let meta = &WIDGET_API_META;
    let find = |name: &str| meta.routes.iter().find(|m| m.method_name == name).unwrap();

    assert_eq!(find("widget.list_widgets").path_template, "/widget");
    assert_eq!(find("widget.get_widget").path_template, "/widget/{id}");
    assert_eq!(find("widget.create_widget").path_template, "/widget");
    assert_eq!(find("widget.delete_widget").path_template, "/widget/{id}");
    assert_eq!(find("widget.rename_widget").path_template, "/widget/{id}");
    assert_eq!(find("widget.send_command").path_template, "/widget/{id}/command");
    assert_eq!(find("widget.kill").path_template, "/widget/kill");
}

#[test]
fn rest_meta_doc_comments_extracted() {
    let meta = &WIDGET_API_META;
    let find = |name: &str| meta.routes.iter().find(|m| m.method_name == name).unwrap();

    assert_eq!(find("widget.list_widgets").description, Some("List all widgets"));
    assert_eq!(find("widget.get_widget").description, Some("Get a widget by ID"));
    assert_eq!(find("widget.create_widget").description, Some("Create a new widget"));
    assert_eq!(find("widget.kill").description, Some("Kill the service (excluded from tools)"));
}

#[test]
fn rest_meta_no_tool_flag() {
    let meta = &WIDGET_API_META;
    let find = |name: &str| meta.routes.iter().find(|m| m.method_name == name).unwrap();

    assert!(!find("widget.list_widgets").no_tool);
    assert!(!find("widget.get_widget").no_tool);
    assert!(find("widget.kill").no_tool, "kill should have no_tool = true");
}

// ===========================================================================
// PART 2: RestDispatcher tests (matchit routing)
// ===========================================================================

#[tokio::test]
async fn rest_dispatch_get_collection() {
    let rd = make_rd_from_widgets(vec![
        Widget { id: "1".into(), label: "A".into() },
        Widget { id: "2".into(), label: "B".into() },
    ]);
    let result = rd.dispatch(HttpMethod::Get, "/widget", Value::Null).await.map(|rr| rr.result);
    let items: Vec<Widget> = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn rest_dispatch_get_single() {
    let rd = make_rd_from_widgets(vec![Widget { id: "abc".into(), label: "Thing".into() }]);
    let result = rd.dispatch(HttpMethod::Get, "/widget/abc", Value::Null).await.map(|rr| rr.result);
    let widget: Widget = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(widget.id, "abc");
}

#[tokio::test]
async fn rest_dispatch_post_body() {
    let rd = make_rd_from_widgets(vec![]);
    let result = rd.dispatch(HttpMethod::Post, "/widget", json!({"id": "new", "label": "New Widget"})).await.map(|rr| rr.result);
    let widget: Widget = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(widget.id, "new");
}

#[tokio::test]
async fn rest_dispatch_delete() {
    let rd = make_rd_from_widgets(vec![Widget { id: "del".into(), label: "D".into() }]);
    let result = rd.dispatch(HttpMethod::Delete, "/widget/del", Value::Null).await.map(|rr| rr.result);
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));
}

#[tokio::test]
async fn rest_dispatch_put_path_and_body() {
    let rd = make_rd_from_widgets(vec![Widget { id: "1".into(), label: "Old".into() }]);
    let result = rd.dispatch(HttpMethod::Put, "/widget/1", json!({"label": "New"})).await.map(|rr| rr.result);
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));
    let result = rd.dispatch(HttpMethod::Get, "/widget/1", Value::Null).await.map(|rr| rr.result);
    let w: Widget = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(w.label, "New");
}

#[tokio::test]
async fn rest_dispatch_nested_path() {
    let rd = make_rd_from_widgets(vec![]);
    let result = rd.dispatch(HttpMethod::Post, "/widget/w1/command", json!({"action": "restart"})).await.map(|rr| rr.result);
    let r: String = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(r, "w1:restart");
}

#[tokio::test]
async fn rest_dispatch_post_no_params() {
    let rd = make_rd_from_widgets(vec![]);
    let result = rd.dispatch(HttpMethod::Post, "/widget/kill", Value::Null).await.map(|rr| rr.result);
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));
}

#[tokio::test]
async fn rest_dispatch_unknown_path_returns_none() {
    let rd = make_rd_from_widgets(vec![]);
    assert!(rd.dispatch(HttpMethod::Get, "/other/path", Value::Null).await.is_none());
}

// ===========================================================================
// PART 3: Client macro tests (WS dispatch round-trip)
// ===========================================================================

struct MockWsClient {
    rd: RestDispatcher,
    svc: Arc<WidgetApiService<dyn WidgetApi>>,
}

impl MockWsClient {
    fn new(widgets: Vec<Widget>) -> Self {
        let impl_ = Arc::new(InMemoryWidgets::new(widgets));
        let svc = <dyn WidgetApi>::service(impl_);
        let rd = RestDispatcher::new().register(svc.clone());
        Self { rd, svc }
    }
}

#[async_trait]
impl RpcClient for MockWsClient {
    type Stream = tokio::sync::broadcast::Receiver<String>;

    async fn rpc_call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        match self.svc.dispatch(method, params).await {
            Some(dr) => dr.result,
            None => Err(anyhow::anyhow!("unknown method: {method}")),
        }
    }

    async fn register_stream(&self, _id: &str) -> Self::Stream {
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        drop(tx);
        rx
    }

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

impl_remote_widget_api!(MockWsClient);

#[tokio::test]
async fn client_list_widgets() {
    let client = MockWsClient::new(vec![
        Widget { id: "a".into(), label: "A".into() },
    ]);
    let items = client.list_widgets().await.unwrap();
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn client_get_widget() {
    let client = MockWsClient::new(vec![Widget { id: "x".into(), label: "X".into() }]);
    let w = client.get_widget("x").await.unwrap();
    assert_eq!(w.label, "X");
}

#[tokio::test]
async fn client_create_and_list() {
    let client = MockWsClient::new(vec![]);
    client.create_widget(Widget { id: "n".into(), label: "N".into() }).await.unwrap();
    assert_eq!(client.list_widgets().await.unwrap().len(), 1);
}

#[tokio::test]
async fn client_delete_widget() {
    let client = MockWsClient::new(vec![Widget { id: "d".into(), label: "D".into() }]);
    client.delete_widget("d").await.unwrap();
    assert!(client.list_widgets().await.unwrap().is_empty());
}

#[tokio::test]
async fn client_rename_widget() {
    let client = MockWsClient::new(vec![Widget { id: "1".into(), label: "Old".into() }]);
    client.rename_widget("1", "New").await.unwrap();
    assert_eq!(client.get_widget("1").await.unwrap().label, "New");
}

#[tokio::test]
async fn client_send_command() {
    let client = MockWsClient::new(vec![]);
    assert_eq!(client.send_command("abc", "reboot").await.unwrap(), "abc:reboot");
}

#[tokio::test]
async fn client_skip_method_errors() {
    let client = MockWsClient::new(vec![]);
    let err = client.internal_op().await.unwrap_err();
    assert!(err.to_string().contains("not available over RPC"));
}

// ===========================================================================
// PART 4: Raw HTTP integration tests — real server, reqwest client
//
// Zero simply-rpc code on the client side. Only reqwest + serde_json.
// ===========================================================================

/// Start a real HTTP server backed by `RestDispatcher` on an OS-assigned port.
/// Returns the base URL (e.g. "http://127.0.0.1:12345").
async fn start_test_server(widgets: Vec<Widget>) -> String {
    use axum::extract::State;
    use axum::http::{Method, StatusCode};
    use axum::response::{IntoResponse, Json as AxumJson};
    use axum::Router;

    let impl_ = Arc::new(InMemoryWidgets::new(widgets));
    let svc = <dyn WidgetApi>::service(impl_);
    let dispatcher = Arc::new(RestDispatcher::new().register(svc));

    async fn handler(
        State(rd): State<Arc<RestDispatcher>>,
        req: axum::extract::Request,
    ) -> axum::response::Response {
        let method = req.method().clone();
        let path = req.uri().path().to_string();

        let http_method = match method {
            Method::GET => HttpMethod::Get,
            Method::POST => HttpMethod::Post,
            Method::PUT => HttpMethod::Put,
            Method::DELETE => HttpMethod::Delete,
            _ => return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response(),
        };

        let body = match http_method {
            HttpMethod::Post | HttpMethod::Put => {
                let bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024).await.unwrap_or_default();
                if bytes.is_empty() { Value::Null }
                else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) }
            }
            _ => Value::Null,
        };

        match rd.dispatch(http_method, &path, body).await {
            Some(rr) => match rr.result {
                Ok(value) => AxumJson(value).into_response(),
                Err(e) => {
                    let msg = e.to_string();
                    let status = if msg.contains("not found") { StatusCode::NOT_FOUND } else { StatusCode::INTERNAL_SERVER_ERROR };
                    (status, msg).into_response()
                }
            },
            None => (StatusCode::NOT_FOUND, "not found").into_response(),
        }
    }

    let app = Router::new()
        .fallback(handler)
        .with_state(dispatcher);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    format!("http://127.0.0.1:{port}")
}

#[tokio::test]
async fn http_get_list() {
    let base = start_test_server(vec![
        Widget { id: "1".into(), label: "A".into() },
        Widget { id: "2".into(), label: "B".into() },
    ]).await;

    let resp = reqwest::get(format!("{base}/widget")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let items: Vec<Widget> = resp.json().await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, "1");
    assert_eq!(items[1].id, "2");
}

#[tokio::test]
async fn http_get_by_id() {
    let base = start_test_server(vec![
        Widget { id: "abc".into(), label: "Thing".into() },
    ]).await;

    let resp = reqwest::get(format!("{base}/widget/abc")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let widget: Widget = resp.json().await.unwrap();
    assert_eq!(widget.id, "abc");
    assert_eq!(widget.label, "Thing");
}

#[tokio::test]
async fn http_get_not_found() {
    let base = start_test_server(vec![]).await;

    let resp = reqwest::get(format!("{base}/widget/missing")).await.unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn http_post_create() {
    let base = start_test_server(vec![]).await;

    let client = reqwest::Client::new();
    let resp = client.post(format!("{base}/widget"))
        .json(&json!({"id": "new", "label": "New Widget"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let widget: Widget = resp.json().await.unwrap();
    assert_eq!(widget.id, "new");
    assert_eq!(widget.label, "New Widget");

    // Verify it persisted
    let resp = reqwest::get(format!("{base}/widget")).await.unwrap();
    let items: Vec<Widget> = resp.json().await.unwrap();
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn http_delete() {
    let base = start_test_server(vec![
        Widget { id: "del".into(), label: "D".into() },
    ]).await;

    let client = reqwest::Client::new();
    let resp = client.delete(format!("{base}/widget/del")).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    // Verify deleted
    let resp = reqwest::get(format!("{base}/widget")).await.unwrap();
    let items: Vec<Widget> = resp.json().await.unwrap();
    assert!(items.is_empty());
}

#[tokio::test]
async fn http_put_rename() {
    let base = start_test_server(vec![
        Widget { id: "1".into(), label: "Old".into() },
    ]).await;

    let client = reqwest::Client::new();
    let resp = client.put(format!("{base}/widget/1"))
        .json(&json!({"label": "New"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);

    // Verify renamed
    let resp = reqwest::get(format!("{base}/widget/1")).await.unwrap();
    let widget: Widget = resp.json().await.unwrap();
    assert_eq!(widget.label, "New");
}

#[tokio::test]
async fn http_post_nested_path() {
    let base = start_test_server(vec![]).await;

    let client = reqwest::Client::new();
    let resp = client.post(format!("{base}/widget/w1/command"))
        .json(&json!({"action": "restart"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let result: String = resp.json().await.unwrap();
    assert_eq!(result, "w1:restart");
}

#[tokio::test]
async fn http_post_no_body() {
    let base = start_test_server(vec![]).await;

    let client = reqwest::Client::new();
    let resp = client.post(format!("{base}/widget/kill")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn http_unknown_path_404() {
    let base = start_test_server(vec![]).await;

    let resp = reqwest::get(format!("{base}/unknown/path")).await.unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn http_delete_not_found() {
    let base = start_test_server(vec![]).await;

    let client = reqwest::Client::new();
    let resp = client.delete(format!("{base}/widget/nope")).send().await.unwrap();
    // Should be 404 or 500 with "not found" message
    assert_ne!(resp.status(), 200);
}

#[tokio::test]
async fn http_full_crud_sequence() {
    let base = start_test_server(vec![]).await;
    let client = reqwest::Client::new();

    // Create two widgets
    client.post(format!("{base}/widget"))
        .json(&json!({"id": "a", "label": "Alpha"}))
        .send().await.unwrap();
    client.post(format!("{base}/widget"))
        .json(&json!({"id": "b", "label": "Beta"}))
        .send().await.unwrap();

    // List: should have 2
    let items: Vec<Widget> = reqwest::get(format!("{base}/widget"))
        .await.unwrap().json().await.unwrap();
    assert_eq!(items.len(), 2);

    // Rename b
    client.put(format!("{base}/widget/b"))
        .json(&json!({"label": "Bravo"}))
        .send().await.unwrap();
    let w: Widget = reqwest::get(format!("{base}/widget/b"))
        .await.unwrap().json().await.unwrap();
    assert_eq!(w.label, "Bravo");

    // Delete a
    client.delete(format!("{base}/widget/a")).send().await.unwrap();
    let items: Vec<Widget> = reqwest::get(format!("{base}/widget"))
        .await.unwrap().json().await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "b");
}

// ===========================================================================
// PART 5: WS dispatch still works for stream/non-REST methods
// ===========================================================================

#[tokio::test]
async fn ws_dispatch_still_works_for_stream() {
    let svc = make_svc(vec![Widget { id: "1".into(), label: "A".into() }]);
    let dr = svc.dispatch("widget.watch_widgets", Value::Null).await;
    assert!(dr.is_some());
    assert!(!dr.unwrap().streams.is_empty());
}

#[tokio::test]
async fn ws_dispatch_still_works_for_non_rest() {
    let svc = make_svc(vec![Widget { id: "1".into(), label: "A".into() }]);
    let dr = svc.dispatch("widget.widget_count", Value::Null).await;
    let count: usize = serde_json::from_value(dr.unwrap().result.unwrap()).unwrap();
    assert_eq!(count, 1);
}
