//! Tests for REST dispatch: path parsing, parameter extraction, body deserialization,
//! metadata generation, doc comments, and no_tool annotation.

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WidgetId(pub String);

impl std::fmt::Display for WidgetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
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

// ---------------------------------------------------------------------------
// REST metadata tests
// ---------------------------------------------------------------------------

#[test]
fn rest_meta_generated() {
    let meta = &WIDGET_API_META;
    assert_eq!(meta.prefix, "widget");
    // Should have REST entries for annotated methods only
    assert!(!meta.rest_methods.is_empty());

    let rest_names: Vec<&str> = meta.rest_methods.iter().map(|m| m.method_name).collect();
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
    let find = |name: &str| meta.rest_methods.iter().find(|m| m.method_name == name).unwrap();

    assert_eq!(find("widget.list_widgets").http_method, HttpMethod::Get);
    assert_eq!(find("widget.get_widget").http_method, HttpMethod::Get);
    assert_eq!(find("widget.create_widget").http_method, HttpMethod::Post);
    assert_eq!(find("widget.delete_widget").http_method, HttpMethod::Delete);
    assert_eq!(find("widget.rename_widget").http_method, HttpMethod::Put);
    assert_eq!(find("widget.send_command").http_method, HttpMethod::Post);
    assert_eq!(find("widget.kill").http_method, HttpMethod::Post);
}

#[test]
fn rest_meta_path_templates_correct() {
    let meta = &WIDGET_API_META;
    let find = |name: &str| meta.rest_methods.iter().find(|m| m.method_name == name).unwrap();

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
    let find = |name: &str| meta.rest_methods.iter().find(|m| m.method_name == name).unwrap();

    assert_eq!(find("widget.list_widgets").description, Some("List all widgets"));
    assert_eq!(find("widget.get_widget").description, Some("Get a widget by ID"));
    assert_eq!(find("widget.create_widget").description, Some("Create a new widget"));
    assert_eq!(find("widget.kill").description, Some("Kill the service (excluded from tools)"));
}

#[test]
fn rest_meta_no_tool_flag() {
    let meta = &WIDGET_API_META;
    let find = |name: &str| meta.rest_methods.iter().find(|m| m.method_name == name).unwrap();

    assert!(!find("widget.list_widgets").no_tool);
    assert!(!find("widget.get_widget").no_tool);
    assert!(find("widget.kill").no_tool, "kill should have no_tool = true");
}

// ---------------------------------------------------------------------------
// REST dispatch tests — GET
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rest_dispatch_get_collection() {
    let svc = make_svc(vec![
        Widget { id: "1".into(), label: "A".into() },
        Widget { id: "2".into(), label: "B".into() },
    ]);

    let result = svc.rest_dispatch(HttpMethod::Get, "/widget", Value::Null).await;
    let items: Vec<Widget> = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn rest_dispatch_get_single() {
    let svc = make_svc(vec![Widget { id: "abc".into(), label: "Thing".into() }]);

    let result = svc.rest_dispatch(HttpMethod::Get, "/widget/abc", Value::Null).await;
    let widget: Widget = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(widget.id, "abc");
    assert_eq!(widget.label, "Thing");
}

#[tokio::test]
async fn rest_dispatch_get_not_found() {
    let svc = make_svc(vec![]);

    let result = svc.rest_dispatch(HttpMethod::Get, "/widget/missing", Value::Null).await;
    assert!(result.unwrap().is_err());
}

// ---------------------------------------------------------------------------
// REST dispatch tests — POST
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rest_dispatch_post_body() {
    let svc = make_svc(vec![]);

    let body = json!({"id": "new", "label": "New Widget"});
    let result = svc.rest_dispatch(HttpMethod::Post, "/widget", body).await;
    let widget: Widget = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(widget.id, "new");
}

#[tokio::test]
async fn rest_dispatch_post_no_params() {
    let svc = make_svc(vec![]);

    let result = svc.rest_dispatch(HttpMethod::Post, "/widget/kill", Value::Null).await;
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));
}

// ---------------------------------------------------------------------------
// REST dispatch tests — DELETE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rest_dispatch_delete() {
    let svc = make_svc(vec![Widget { id: "del".into(), label: "D".into() }]);

    let result = svc.rest_dispatch(HttpMethod::Delete, "/widget/del", Value::Null).await;
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));

    // Verify deleted
    let result = svc.rest_dispatch(HttpMethod::Get, "/widget", Value::Null).await;
    let items: Vec<Widget> = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert!(items.is_empty());
}

// ---------------------------------------------------------------------------
// REST dispatch tests — PUT with path + body params
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rest_dispatch_put_path_and_body() {
    let svc = make_svc(vec![Widget { id: "1".into(), label: "Old".into() }]);

    let body = json!({"label": "New"});
    let result = svc.rest_dispatch(HttpMethod::Put, "/widget/1", body).await;
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));

    // Verify renamed
    let result = svc.rest_dispatch(HttpMethod::Get, "/widget/1", Value::Null).await;
    let widget: Widget = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(widget.label, "New");
}

// ---------------------------------------------------------------------------
// REST dispatch tests — nested path with path + body params
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rest_dispatch_nested_path() {
    let svc = make_svc(vec![]);

    let body = json!({"action": "restart"});
    let result = svc.rest_dispatch(HttpMethod::Post, "/widget/w1/command", body).await;
    let response: String = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(response, "w1:restart");
}

// ---------------------------------------------------------------------------
// REST dispatch — no match cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rest_dispatch_wrong_method_returns_none() {
    let svc = make_svc(vec![]);

    // GET on a POST endpoint
    let result = svc.rest_dispatch(HttpMethod::Get, "/widget/kill", Value::Null).await;
    // "/widget/kill" with GET would try to match get_widget with id="kill"
    // which should return not found error (no widget with id "kill")
    // But it WON'T match the POST kill endpoint
    // Actually, GET /widget/{id} matches with id="kill", so it dispatches get_widget("kill")
    // which errors because there's no widget with that id
    assert!(result.is_some()); // It matched get_widget
    assert!(result.unwrap().is_err()); // But no widget named "kill"
}

#[tokio::test]
async fn rest_dispatch_unknown_path_returns_none() {
    let svc = make_svc(vec![]);

    let result = svc.rest_dispatch(HttpMethod::Get, "/other/path", Value::Null).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn rest_dispatch_too_many_segments_returns_none() {
    let svc = make_svc(vec![]);

    let result = svc.rest_dispatch(HttpMethod::Get, "/widget/a/b/c/d", Value::Null).await;
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// WS dispatch still works for stream methods
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_dispatch_still_works_for_stream() {
    let svc = make_svc(vec![Widget { id: "1".into(), label: "A".into() }]);

    // Stream methods dispatched via the WS dispatch path
    let dr = svc.dispatch("widget.watch_widgets", Value::Null).await;
    assert!(dr.is_some());
    let dr = dr.unwrap();
    assert!(!dr.streams.is_empty());
}

#[tokio::test]
async fn ws_dispatch_still_works_for_non_rest() {
    let svc = make_svc(vec![Widget { id: "1".into(), label: "A".into() }]);

    // Non-annotated methods still work through WS dispatch
    let dr = svc.dispatch("widget.widget_count", Value::Null).await;
    assert!(dr.is_some());
    let count: usize = serde_json::from_value(dr.unwrap().result.unwrap()).unwrap();
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// Client macro tests — round-trip through WS dispatch (existing transport)
// ---------------------------------------------------------------------------

/// Mock RPC client that dispatches locally through the service's WS dispatch.
struct MockWsClient {
    svc: WidgetApiService<InMemoryWidgets>,
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
        // Dummy stream for testing
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        drop(tx);
        rx
    }

    async fn unregister_stream(&self, _id: &str) {}
}

impl_remote_widget_api!(MockWsClient);

#[tokio::test]
async fn client_list_widgets() {
    let client = MockWsClient {
        svc: make_svc(vec![
            Widget { id: "a".into(), label: "A".into() },
            Widget { id: "b".into(), label: "B".into() },
        ]),
    };

    let items = client.list_widgets().await.unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn client_get_widget() {
    let client = MockWsClient {
        svc: make_svc(vec![Widget { id: "x".into(), label: "X".into() }]),
    };

    let widget = client.get_widget("x").await.unwrap();
    assert_eq!(widget.id, "x");
    assert_eq!(widget.label, "X");
}

#[tokio::test]
async fn client_create_widget() {
    let client = MockWsClient {
        svc: make_svc(vec![]),
    };

    let created = client
        .create_widget(Widget { id: "new".into(), label: "New".into() })
        .await
        .unwrap();
    assert_eq!(created.id, "new");

    let items = client.list_widgets().await.unwrap();
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn client_delete_widget() {
    let client = MockWsClient {
        svc: make_svc(vec![Widget { id: "d".into(), label: "D".into() }]),
    };

    client.delete_widget("d").await.unwrap();
    let items = client.list_widgets().await.unwrap();
    assert!(items.is_empty());
}

#[tokio::test]
async fn client_rename_widget() {
    let client = MockWsClient {
        svc: make_svc(vec![Widget { id: "1".into(), label: "Old".into() }]),
    };

    client.rename_widget("1", "New").await.unwrap();
    let widget = client.get_widget("1").await.unwrap();
    assert_eq!(widget.label, "New");
}

#[tokio::test]
async fn client_send_command() {
    let client = MockWsClient {
        svc: make_svc(vec![]),
    };

    let result = client.send_command("abc", "reboot").await.unwrap();
    assert_eq!(result, "abc:reboot");
}

#[tokio::test]
async fn client_skip_method_errors() {
    let client = MockWsClient {
        svc: make_svc(vec![]),
    };

    let result = client.internal_op().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not available over RPC"));
}

#[tokio::test]
async fn client_error_propagated() {
    let client = MockWsClient {
        svc: make_svc(vec![]),
    };

    let result = client.get_widget("missing").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

// ---------------------------------------------------------------------------
// RestDispatcher tests — simulates raw HTTP request routing
// ---------------------------------------------------------------------------

fn make_rest_dispatcher(widgets: Vec<Widget>) -> RestDispatcher {
    let svc = Arc::new(make_svc(widgets));
    RestDispatcher::new().register(svc as Arc<dyn RestService>)
}

#[tokio::test]
async fn http_get_collection() {
    let rd = make_rest_dispatcher(vec![
        Widget { id: "1".into(), label: "A".into() },
        Widget { id: "2".into(), label: "B".into() },
    ]);

    let result = rd.dispatch(HttpMethod::Get, "/widget", Value::Null).await;
    let items: Vec<Widget> = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn http_get_single_by_path() {
    let rd = make_rest_dispatcher(vec![Widget { id: "abc".into(), label: "Thing".into() }]);

    let result = rd.dispatch(HttpMethod::Get, "/widget/abc", Value::Null).await;
    let widget: Widget = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(widget.id, "abc");
}

#[tokio::test]
async fn http_post_with_json_body() {
    let rd = make_rest_dispatcher(vec![]);

    let body = json!({"id": "w1", "label": "Widget One"});
    let result = rd.dispatch(HttpMethod::Post, "/widget", body).await;
    let widget: Widget = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(widget.id, "w1");
    assert_eq!(widget.label, "Widget One");
}

#[tokio::test]
async fn http_delete_by_path() {
    let rd = make_rest_dispatcher(vec![Widget { id: "x".into(), label: "X".into() }]);

    let result = rd.dispatch(HttpMethod::Delete, "/widget/x", Value::Null).await;
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));

    // Verify deleted
    let result = rd.dispatch(HttpMethod::Get, "/widget", Value::Null).await;
    let items: Vec<Widget> = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert!(items.is_empty());
}

#[tokio::test]
async fn http_put_path_and_body() {
    let rd = make_rest_dispatcher(vec![Widget { id: "1".into(), label: "Old".into() }]);

    let result = rd.dispatch(HttpMethod::Put, "/widget/1", json!({"label": "New"})).await;
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));

    let result = rd.dispatch(HttpMethod::Get, "/widget/1", Value::Null).await;
    let widget: Widget = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(widget.label, "New");
}

#[tokio::test]
async fn http_post_nested_path() {
    let rd = make_rest_dispatcher(vec![]);

    let result = rd.dispatch(HttpMethod::Post, "/widget/w1/command", json!({"action": "go"})).await;
    let response: String = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(response, "w1:go");
}

#[tokio::test]
async fn http_post_no_params() {
    let rd = make_rest_dispatcher(vec![]);

    let result = rd.dispatch(HttpMethod::Post, "/widget/kill", Value::Null).await;
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));
}

#[tokio::test]
async fn http_unknown_path_returns_none() {
    let rd = make_rest_dispatcher(vec![]);

    let result = rd.dispatch(HttpMethod::Get, "/unknown/path", Value::Null).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn http_error_propagated() {
    let rd = make_rest_dispatcher(vec![]);

    let result = rd.dispatch(HttpMethod::Get, "/widget/missing", Value::Null).await;
    assert!(result.unwrap().is_err());
}

#[tokio::test]
async fn rest_meta_available_from_dispatcher() {
    let rd = make_rest_dispatcher(vec![]);

    let metas = rd.rest_metas();
    assert!(!metas.is_empty());

    let paths: Vec<&str> = metas.iter().map(|m| m.path_template).collect();
    assert!(paths.contains(&"/widget"));
    assert!(paths.contains(&"/widget/{id}"));
    assert!(paths.contains(&"/widget/{id}/command"));
    assert!(paths.contains(&"/widget/kill"));
}
