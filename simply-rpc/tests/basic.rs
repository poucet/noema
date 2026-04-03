//! Tests for REST RPC: server dispatch + client macro generation.
//!
//! Covers: Result<()>, Result<T>, raw T, single param, multi param, &str, &T, skip.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simply_rpc::{HttpMethod, RestDispatcher, RpcClient, RpcService, check_compat};

// ---------------------------------------------------------------------------
// Test trait — exercises all non-streaming patterns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub id: String,
    pub name: String,
}

#[simply_rpc::rpc_service("items")]
#[async_trait]
pub trait ItemApi: Send + Sync {
    /// List all items
    #[rpc(get = "/items")]
    async fn list_items(&self) -> anyhow::Result<Vec<Item>>;

    /// Get an item by ID
    #[rpc(get = "/items/{id}")]
    async fn get_item(&self, id: &str) -> anyhow::Result<Item>;

    /// Add a new item
    #[rpc(post = "/items")]
    async fn add_item(&self, item: Item) -> anyhow::Result<()>;

    /// Rename an item (path + body params)
    #[rpc(put = "/items/{id}")]
    async fn rename_item(&self, id: &str, name: &str) -> anyhow::Result<()>;

    /// Count items (raw return, no Result)
    #[rpc(get = "/items/count")]
    async fn count_items(&self) -> usize;

    /// Skipped method
    #[rpc(skip)]
    async fn dangerous(&self) -> anyhow::Result<()>;
}

// ---------------------------------------------------------------------------
// In-memory implementation (server side)
// ---------------------------------------------------------------------------

struct InMemoryItems {
    items: tokio::sync::Mutex<Vec<Item>>,
}

impl InMemoryItems {
    fn new(items: Vec<Item>) -> Self {
        Self {
            items: tokio::sync::Mutex::new(items),
        }
    }
}

#[async_trait]
impl ItemApi for InMemoryItems {
    async fn list_items(&self) -> anyhow::Result<Vec<Item>> {
        Ok(self.items.lock().await.clone())
    }

    async fn get_item(&self, id: &str) -> anyhow::Result<Item> {
        self.items
            .lock()
            .await
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("not found: {id}"))
    }

    async fn add_item(&self, item: Item) -> anyhow::Result<()> {
        self.items.lock().await.push(item);
        Ok(())
    }

    async fn rename_item(&self, id: &str, name: &str) -> anyhow::Result<()> {
        let mut items = self.items.lock().await;
        let item = items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| anyhow::anyhow!("not found: {id}"))?;
        item.name = name.to_string();
        Ok(())
    }

    async fn count_items(&self) -> usize {
        self.items.lock().await.len()
    }

    async fn dangerous(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn make_rd(items: Vec<Item>) -> RestDispatcher {
    let impl_ = Arc::new(InMemoryItems::new(items));
    let svc = <dyn ItemApi>::service(impl_);
    RestDispatcher::new().register(svc)
}

// ---------------------------------------------------------------------------
// REST dispatch tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_list_no_params() {
    let rd = make_rd(vec![
        Item { id: "1".into(), name: "Alpha".into() },
        Item { id: "2".into(), name: "Beta".into() },
    ]);
    let result = rd.dispatch(HttpMethod::Get, "/items", Value::Null).await;
    let items: Vec<Item> = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "Alpha");
}

#[tokio::test]
async fn dispatch_single_str_ref_param() {
    let rd = make_rd(vec![Item { id: "abc".into(), name: "Thing".into() }]);
    let result = rd.dispatch(HttpMethod::Get, "/items/abc", Value::Null).await;
    let item: Item = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(item.id, "abc");
    assert_eq!(item.name, "Thing");
}

#[tokio::test]
async fn dispatch_owned_param() {
    let rd = make_rd(vec![]);
    let result = rd.dispatch(HttpMethod::Post, "/items", json!({"id": "x", "name": "X"})).await;
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));

    let result = rd.dispatch(HttpMethod::Get, "/items", Value::Null).await;
    let items: Vec<Item> = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "x");
}

#[tokio::test]
async fn dispatch_multi_params() {
    let rd = make_rd(vec![Item { id: "1".into(), name: "Old".into() }]);
    let result = rd.dispatch(HttpMethod::Put, "/items/1", json!({"name": "New"})).await;
    assert_eq!(result.unwrap().unwrap(), Value::Bool(true));

    let result = rd.dispatch(HttpMethod::Get, "/items/1", Value::Null).await;
    let item: Item = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(item.name, "New");
}

#[tokio::test]
async fn dispatch_raw_return() {
    let rd = make_rd(vec![
        Item { id: "1".into(), name: "A".into() },
        Item { id: "2".into(), name: "B".into() },
    ]);
    let result = rd.dispatch(HttpMethod::Get, "/items/count", Value::Null).await;
    let count: usize = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn dispatch_unknown_path_returns_none() {
    let rd = make_rd(vec![]);
    assert!(rd.dispatch(HttpMethod::Get, "/other/path", Value::Null).await.is_none());
}

#[tokio::test]
async fn dispatch_skip_method_not_dispatched() {
    let rd = make_rd(vec![]);
    // Skip methods have no REST annotation, so no path matches
    assert!(rd.dispatch(HttpMethod::Post, "/items/dangerous", Value::Null).await.is_none());
}

#[tokio::test]
async fn dispatch_error_propagated() {
    let rd = make_rd(vec![]);
    let result = rd.dispatch(HttpMethod::Get, "/items/missing", Value::Null).await;
    assert!(result.unwrap().is_err());
}

#[tokio::test]
async fn dispatch_bad_params_returns_error() {
    let rd = make_rd(vec![]);
    // rename_item expects {name} in body but we send null
    let result = rd.dispatch(HttpMethod::Put, "/items/1", Value::Null).await;
    assert!(result.unwrap().is_err());
}

// ---------------------------------------------------------------------------
// RestDispatcher tests
// ---------------------------------------------------------------------------

fn make_rest_dispatcher(items: Vec<Item>) -> RestDispatcher {
    let impl_ = Arc::new(InMemoryItems::new(items));
    let svc = <dyn ItemApi>::service(impl_);
    RestDispatcher::new().register(svc)
}

#[tokio::test]
async fn rest_dispatcher_routes_correctly() {
    let rd = make_rest_dispatcher(vec![Item { id: "1".into(), name: "One".into() }]);
    let result = rd.dispatch(HttpMethod::Get, "/items/count", Value::Null).await;
    let count: usize = serde_json::from_value(result.unwrap().unwrap()).unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn rest_dispatcher_unknown_path_returns_none() {
    let rd = make_rest_dispatcher(vec![]);
    let result = rd.dispatch(HttpMethod::Get, "/nope/nothing", Value::Null).await;
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// Client macro tests (round-trip through RestDispatcher)
// ---------------------------------------------------------------------------

struct MockClient {
    rd: RestDispatcher,
}

impl MockClient {
    fn new(items: Vec<Item>) -> Self {
        Self { rd: make_rd(items) }
    }
}

#[async_trait]
impl RpcClient for MockClient {
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

impl_remote_item_api!(MockClient);

#[tokio::test]
async fn client_list_items() {
    let client = MockClient::new(vec![
        Item { id: "a".into(), name: "A".into() },
    ]);
    let items = client.list_items().await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "A");
}

#[tokio::test]
async fn client_get_item() {
    let client = MockClient::new(vec![
        Item { id: "x".into(), name: "X".into() },
    ]);
    let item = client.get_item("x").await.unwrap();
    assert_eq!(item.id, "x");
}

#[tokio::test]
async fn client_add_item() {
    let client = MockClient::new(vec![]);
    client.add_item(Item { id: "new".into(), name: "New".into() }).await.unwrap();
    let items = client.list_items().await.unwrap();
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn client_rename_item() {
    let client = MockClient::new(vec![
        Item { id: "1".into(), name: "Old".into() },
    ]);
    client.rename_item("1", "New").await.unwrap();
    let item = client.get_item("1").await.unwrap();
    assert_eq!(item.name, "New");
}

#[tokio::test]
async fn client_raw_return() {
    let client = MockClient::new(vec![
        Item { id: "1".into(), name: "A".into() },
    ]);
    let count = client.count_items().await;
    assert_eq!(count, 1);
}

#[tokio::test]
async fn client_skip_method_returns_error() {
    let client = MockClient::new(vec![]);
    let result = client.dangerous().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not available over RPC"));
}

#[tokio::test]
async fn client_error_propagated() {
    let client = MockClient::new(vec![]);
    let result = client.get_item("missing").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

// ---------------------------------------------------------------------------
// Compatibility metadata tests
// ---------------------------------------------------------------------------

#[test]
fn meta_generated_for_service() {
    assert_eq!(ITEM_API_META.prefix, "items");
    let names: Vec<&str> = ITEM_API_META.methods.iter().map(|m| m.name).collect();
    assert!(names.contains(&"items.list_items"));
    assert!(names.contains(&"items.get_item"));
    assert!(names.contains(&"items.add_item"));
    assert!(names.contains(&"items.rename_item"));
    assert!(names.contains(&"items.count_items"));
    assert!(!names.contains(&"items.dangerous"));
}

#[test]
fn rest_meta_generated() {
    let paths: Vec<&str> = ITEM_API_META.rest_methods.iter().map(|m| m.path_template).collect();
    assert!(paths.contains(&"/items"));
    assert!(paths.contains(&"/items/{id}"));
    assert!(paths.contains(&"/items/count"));
}

#[test]
fn compat_identical_is_compatible() {
    let client = vec![ITEM_API_META.to_wire()];
    let result = check_compat(&client, &[&ITEM_API_META]);
    assert!(result.compatible, "{result}");
}

#[test]
fn compat_server_superset_is_compatible() {
    let mut client_wire = ITEM_API_META.to_wire();
    client_wire.methods.retain(|m| m.name == "items.list_items");
    let result = check_compat(&[client_wire], &[&ITEM_API_META]);
    assert!(result.compatible, "{result}");
}

#[test]
fn compat_missing_method_detected() {
    use simply_rpc::meta::MethodMetaWire;
    let client_wire = simply_rpc::ServiceMetaWire {
        prefix: "items".into(),
        methods: vec![MethodMetaWire {
            name: "items.nonexistent".into(),
            signature_hash: 0,
        }],
    };
    let result = check_compat(&[client_wire], &[&ITEM_API_META]);
    assert!(!result.compatible);
    assert!(result.missing.contains(&"items.nonexistent".to_string()));
}

#[test]
fn compat_signature_mismatch_detected() {
    use simply_rpc::meta::MethodMetaWire;
    let client_wire = simply_rpc::ServiceMetaWire {
        prefix: "items".into(),
        methods: vec![MethodMetaWire {
            name: "items.list_items".into(),
            signature_hash: 99999,
        }],
    };
    let result = check_compat(&[client_wire], &[&ITEM_API_META]);
    assert!(!result.compatible);
    assert!(result.mismatched.contains(&"items.list_items".to_string()));
}

#[test]
fn compat_missing_prefix_detected() {
    use simply_rpc::meta::MethodMetaWire;
    let client_wire = simply_rpc::ServiceMetaWire {
        prefix: "unknown".into(),
        methods: vec![MethodMetaWire {
            name: "unknown.foo".into(),
            signature_hash: 0,
        }],
    };
    let result = check_compat(&[client_wire], &[&ITEM_API_META]);
    assert!(!result.compatible);
    assert!(result.missing.contains(&"unknown.foo".to_string()));
}
