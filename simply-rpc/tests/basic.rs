//! Tests for non-streaming RPC: server dispatch + client macro generation.
//!
//! Covers: Result<()>, Result<T>, raw T, single param, multi param, &str, &T, skip.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simply_rpc::{Dispatcher, RpcClient, RpcService, check_compat};

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
    /// No params, returns Result<Vec<T>>
    async fn list_items(&self) -> anyhow::Result<Vec<Item>>;

    /// Single &str param, returns Result<T>
    async fn get_item(&self, id: &str) -> anyhow::Result<Item>;

    /// Owned param, returns Result<()>
    async fn add_item(&self, item: Item) -> anyhow::Result<()>;

    /// Multi params with refs, returns Result<()>
    async fn rename_item(&self, id: &str, name: &str) -> anyhow::Result<()>;

    /// No Result wrapper — raw return
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

// ---------------------------------------------------------------------------
// Server dispatch tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_list_no_params() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![
        Item { id: "1".into(), name: "Alpha".into() },
        Item { id: "2".into(), name: "Beta".into() },
    ])));

    let dr = svc.dispatch("items.list_items", Value::Null).await.unwrap();
    let items: Vec<Item> = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "Alpha");
    assert!(dr.streams.is_empty());
}

#[tokio::test]
async fn dispatch_single_str_ref_param() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![
        Item { id: "abc".into(), name: "Thing".into() },
    ])));

    let dr = svc.dispatch("items.get_item", json!("abc")).await.unwrap();
    let item: Item = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert_eq!(item.id, "abc");
    assert_eq!(item.name, "Thing");
}

#[tokio::test]
async fn dispatch_owned_param() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![])));

    let dr = svc
        .dispatch("items.add_item", json!({"id": "x", "name": "X"}))
        .await
        .unwrap();
    assert_eq!(dr.result.unwrap(), Value::Bool(true));

    // Verify it was added
    let dr = svc.dispatch("items.list_items", Value::Null).await.unwrap();
    let items: Vec<Item> = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "x");
}

#[tokio::test]
async fn dispatch_multi_params() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![
        Item { id: "1".into(), name: "Old".into() },
    ])));

    let dr = svc
        .dispatch("items.rename_item", json!({"id": "1", "name": "New"}))
        .await
        .unwrap();
    assert_eq!(dr.result.unwrap(), Value::Bool(true));

    // Verify rename
    let dr = svc.dispatch("items.get_item", json!("1")).await.unwrap();
    let item: Item = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert_eq!(item.name, "New");
}

#[tokio::test]
async fn dispatch_raw_return() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![
        Item { id: "1".into(), name: "A".into() },
        Item { id: "2".into(), name: "B".into() },
    ])));

    let dr = svc.dispatch("items.count_items", Value::Null).await.unwrap();
    let count: usize = serde_json::from_value(dr.result.unwrap()).unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn dispatch_unknown_method_returns_none() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![])));
    assert!(svc.dispatch("items.nonexistent", Value::Null).await.is_none());
}

#[tokio::test]
async fn dispatch_skip_method_not_dispatched() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![])));
    assert!(svc.dispatch("items.dangerous", Value::Null).await.is_none());
}

#[tokio::test]
async fn dispatch_error_propagated() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![])));
    let dr = svc.dispatch("items.get_item", json!("missing")).await.unwrap();
    assert!(dr.result.is_err());
    let err = dr.result.unwrap_err().to_string();
    assert!(err.contains("not found"), "got: {err}");
}

#[tokio::test]
async fn dispatch_bad_params_returns_error() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![])));
    // rename_item expects {id, name} but we send a bare string
    let dr = svc.dispatch("items.rename_item", json!("bad")).await.unwrap();
    assert!(dr.result.is_err());
    let err = dr.result.unwrap_err().to_string();
    assert!(err.contains("deserialize"), "got: {err}");
}

#[tokio::test]
async fn dispatch_prefix_routing() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![])));
    assert_eq!(svc.prefix(), "items");
    // Wrong prefix → None
    assert!(svc.dispatch("other.list_items", Value::Null).await.is_none());
}

// ---------------------------------------------------------------------------
// Dispatcher tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatcher_routes_correctly() {
    let svc: Arc<dyn RpcService<Stream = ()>> =
        Arc::new(ItemApiService(Arc::new(InMemoryItems::new(vec![
            Item { id: "1".into(), name: "One".into() },
        ]))));

    let dispatcher = Dispatcher::new().register(svc);
    let result = dispatcher.dispatch("items.count_items", Value::Null).await;
    let count: usize = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn dispatcher_unknown_prefix_errors() {
    let svc: Arc<dyn RpcService<Stream = ()>> =
        Arc::new(ItemApiService(Arc::new(InMemoryItems::new(vec![]))));

    let dispatcher = Dispatcher::new().register(svc);
    let result = dispatcher.dispatch("nope.nothing", Value::Null).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unknown method"));
}

#[tokio::test]
async fn dispatcher_unknown_method_on_known_prefix_errors() {
    let svc: Arc<dyn RpcService<Stream = ()>> =
        Arc::new(ItemApiService(Arc::new(InMemoryItems::new(vec![]))));

    let dispatcher = Dispatcher::new().register(svc);
    let result = dispatcher.dispatch("items.nonexistent", Value::Null).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unknown method"));
}

#[test]
#[should_panic(expected = "duplicate")]
fn dispatcher_rejects_duplicate_prefix() {
    let svc1: Arc<dyn RpcService<Stream = ()>> =
        Arc::new(ItemApiService(Arc::new(InMemoryItems::new(vec![]))));
    let svc2: Arc<dyn RpcService<Stream = ()>> =
        Arc::new(ItemApiService(Arc::new(InMemoryItems::new(vec![]))));

    Dispatcher::new().register(svc1).register(svc2);
}

// ---------------------------------------------------------------------------
// Client macro tests (simulated RPC round-trip)
// ---------------------------------------------------------------------------

/// A mock RPC client that dispatches locally through the service.
struct MockClient {
    svc: ItemApiService<InMemoryItems>,
}

#[async_trait]
impl RpcClient for MockClient {
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

// Invoke the generated macro to implement ItemApi for MockClient
impl_remote_item_api!(MockClient);

#[tokio::test]
async fn client_list_items() {
    let client = MockClient {
        svc: ItemApiService(Arc::new(InMemoryItems::new(vec![
            Item { id: "a".into(), name: "A".into() },
        ]))),
    };

    let items = client.list_items().await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "A");
}

#[tokio::test]
async fn client_get_item() {
    let client = MockClient {
        svc: ItemApiService(Arc::new(InMemoryItems::new(vec![
            Item { id: "x".into(), name: "X".into() },
        ]))),
    };

    let item = client.get_item("x").await.unwrap();
    assert_eq!(item.id, "x");
}

#[tokio::test]
async fn client_add_item() {
    let client = MockClient {
        svc: ItemApiService(Arc::new(InMemoryItems::new(vec![]))),
    };

    client
        .add_item(Item { id: "new".into(), name: "New".into() })
        .await
        .unwrap();

    let items = client.list_items().await.unwrap();
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn client_rename_item() {
    let client = MockClient {
        svc: ItemApiService(Arc::new(InMemoryItems::new(vec![
            Item { id: "1".into(), name: "Old".into() },
        ]))),
    };

    client.rename_item("1", "New").await.unwrap();

    let item = client.get_item("1").await.unwrap();
    assert_eq!(item.name, "New");
}

#[tokio::test]
async fn client_raw_return() {
    let client = MockClient {
        svc: ItemApiService(Arc::new(InMemoryItems::new(vec![
            Item { id: "1".into(), name: "A".into() },
        ]))),
    };

    let count = client.count_items().await;
    assert_eq!(count, 1);
}

#[tokio::test]
async fn client_skip_method_returns_error() {
    let client = MockClient {
        svc: ItemApiService(Arc::new(InMemoryItems::new(vec![]))),
    };

    let result = client.dangerous().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not available over RPC"));
}

#[tokio::test]
async fn client_error_propagated() {
    let client = MockClient {
        svc: ItemApiService(Arc::new(InMemoryItems::new(vec![]))),
    };

    let result = client.get_item("missing").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

// ---------------------------------------------------------------------------
// Compatibility metadata tests
// ---------------------------------------------------------------------------

#[test]
fn meta_generated_for_service() {
    // The macro generates ITEM_API_META constant
    assert_eq!(ITEM_API_META.prefix, "items");
    // Should have all non-skip methods
    let names: Vec<&str> = ITEM_API_META.methods.iter().map(|m| m.name).collect();
    assert!(names.contains(&"items.list_items"));
    assert!(names.contains(&"items.get_item"));
    assert!(names.contains(&"items.add_item"));
    assert!(names.contains(&"items.rename_item"));
    assert!(names.contains(&"items.count_items"));
    // Skip methods should NOT be in metadata
    assert!(!names.contains(&"items.dangerous"));
}

#[test]
fn meta_service_method_returns_meta() {
    let svc = ItemApiService(Arc::new(InMemoryItems::new(vec![])));
    let meta = svc.meta();
    assert_eq!(meta.prefix, "items");
    assert_eq!(meta.methods.len(), ITEM_API_META.methods.len());
}

#[test]
fn compat_identical_is_compatible() {
    let client = vec![ITEM_API_META.to_wire()];
    let result = check_compat(&client, &[&ITEM_API_META]);
    assert!(result.compatible, "{result}");
}

#[test]
fn compat_server_superset_is_compatible() {
    // Client only expects a subset of methods
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
    // Same method name but wrong hash
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
