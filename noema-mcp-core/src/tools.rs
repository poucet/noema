//! MCP Tools for Noema Core
//!
//! This server is stateless - the agent enriches tool calls with context
//! (conversation_id, turn_id, etc) before forwarding to this server.

use noema_core::agents::{ExecutionContext, McpAgent, ToolEnricher};
use noema_core::mcp::{McpRegistry, McpToolRegistry};
use noema_core::storage::coordinator::StorageCoordinator;
use noema_core::storage::document_resolver::DocumentResolver;
use noema_core::storage::ids::{ConversationId, SpanId, TurnId, UserId};
use noema_core::storage::session::Session;
use noema_core::storage::traits::StorageTypes;
use noema_core::manager::CommitMode;
use noema_core::{Agent, ConversationContext};

/// Create an enricher that injects execution context for noema-core tools.
fn create_noema_core_enricher() -> ToolEnricher {
    Arc::new(|tool_name, args, context| {
        if tool_name == "spawn_agent" {
            match args {
                serde_json::Value::Object(map) => serde_json::Value::Object(context.inject_into(map)),
                other => other,
            }
        } else {
            args
        }
    })
}

use llm::{ChatMessage, ChatPayload, ContentBlock, create_model};
use rmcp::{
    handler::server::ServerHandler,
    model::*,
    service::{RequestContext, RoleServer},
    ErrorData as McpError,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

/// Arguments for spawn_agent tool
///
/// The `_context` field is injected by the agent and contains system identifiers.
#[derive(Debug, Deserialize)]
struct SpawnAgentArgs {
    /// The task/prompt for the spawned agent
    prompt: String,
    /// Optional system prompt for the spawned agent
    system_prompt: Option<String>,
    /// Optional name for the subconversation
    name: Option<String>,
    /// Execution context (injected by agent)
    #[serde(rename = "_context")]
    context: ExecutionContext,
}

/// MCP Server exposing Noema's core capabilities.
/// Stateless - context is passed in tool arguments by the agent.
#[derive(Clone)]
pub struct NoemaCoreServer {
    inner: Arc<NoemaCoreServerInner>,
}

struct NoemaCoreServerInner {
    coordinator: Arc<dyn CoordinatorOps>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    document_resolver: Arc<dyn DocumentResolver>,
}

/// Type-erased coordinator operations needed for spawn
#[async_trait::async_trait]
trait CoordinatorOps: Send + Sync {
    async fn spawn_subconversation(
        &self,
        parent_id: &ConversationId,
        user_id: &UserId,
        turn_id: &TurnId,
        span_id: Option<&SpanId>,
        name: Option<&str>,
    ) -> anyhow::Result<ConversationId>;

    async fn get_subconversation_result(&self, sub_id: &ConversationId) -> anyhow::Result<Option<String>>;

    async fn run_agent_in_subconversation(
        &self,
        sub_id: &ConversationId,
        prompt: String,
        system_prompt: Option<String>,
        model: Arc<dyn llm::ChatModel + Send + Sync>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
        execution_context: ExecutionContext,
    ) -> anyhow::Result<()>;
}

/// Concrete implementation for a specific StorageTypes
struct ConcreteCoordinator<S: StorageTypes> {
    coordinator: Arc<StorageCoordinator<S>>,
}

#[async_trait::async_trait]
impl<S: StorageTypes> CoordinatorOps for ConcreteCoordinator<S> {
    async fn spawn_subconversation(
        &self,
        parent_id: &ConversationId,
        user_id: &UserId,
        turn_id: &TurnId,
        span_id: Option<&SpanId>,
        name: Option<&str>,
    ) -> anyhow::Result<ConversationId> {
        self.coordinator
            .spawn_subconversation(parent_id, user_id, turn_id, span_id, name)
            .await
    }

    async fn get_subconversation_result(&self, sub_id: &ConversationId) -> anyhow::Result<Option<String>> {
        self.coordinator.get_subconversation_result(sub_id).await
    }

    async fn run_agent_in_subconversation(
        &self,
        sub_id: &ConversationId,
        prompt: String,
        system_prompt: Option<String>,
        model: Arc<dyn llm::ChatModel + Send + Sync>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
        execution_context: ExecutionContext,
    ) -> anyhow::Result<()> {
        // Open session
        let resolved_messages = self.coordinator.open_session(sub_id).await?;
        let mut session = Session::new(self.coordinator.clone(), sub_id.clone());
        for msg in resolved_messages {
            session.add_resolved(msg);
        }

        // Add system prompt if provided
        if let Some(system) = system_prompt {
            let system_message = ChatMessage::system(ChatPayload::new(vec![ContentBlock::Text {
                text: system,
            }]));
            session.add(system_message);
        }

        // Add user prompt
        let user_message = ChatMessage::user(ChatPayload::new(vec![ContentBlock::Text {
            text: prompt,
        }]));
        session.add(user_message);

        // Create agent with enricher for nested spawn calls
        let tool_registry = McpToolRegistry::new(mcp_registry);
        let agent = McpAgent::with_enricher(
            Arc::new(tool_registry),
            5, // Lower max iterations for subconversations
            document_resolver,
            execution_context,
            create_noema_core_enricher(),
        );

        // Save model id before move
        let model_id = model.id().to_string();

        // Run agent
        agent.execute(&mut session, model).await?;

        // Commit messages to storage so get_subconversation_result can find them
        session.commit(Some(&model_id), &CommitMode::NewTurns).await
    }
}

impl NoemaCoreServer {
    /// Create a new NoemaCoreServer (stateless, shared across conversations)
    pub fn new<S: StorageTypes + 'static>(
        coordinator: Arc<StorageCoordinator<S>>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
    ) -> Self {
        Self {
            inner: Arc::new(NoemaCoreServerInner {
                coordinator: Arc::new(ConcreteCoordinator { coordinator }),
                mcp_registry,
                document_resolver,
            }),
        }
    }

    fn get_tools() -> Vec<Tool> {
        fn make_schema(value: serde_json::Value) -> Arc<serde_json::Map<String, serde_json::Value>> {
            match value {
                serde_json::Value::Object(map) => Arc::new(map),
                _ => Arc::new(serde_json::Map::new()),
            }
        }

        // Note: The schema only shows what the LLM provides.
        // The agent enriches calls with conversation_id, user_id, etc.
        vec![Tool {
            name: "spawn_agent".into(),
            title: None,
            description: Some(
                "Spawn a subconversation to handle a complex subtask. The spawned agent runs \
                 independently and returns its result. Use this for tasks that require focused \
                 attention or multiple tool calls that are separate from the main conversation flow."
                    .into(),
            ),
            input_schema: make_schema(json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "The task/prompt for the spawned agent"
                    },
                    "system_prompt": {
                        "type": "string",
                        "description": "Optional system prompt for the spawned agent"
                    },
                    "name": {
                        "type": "string",
                        "description": "Optional name for the subconversation (e.g., 'Research API docs')"
                    }
                },
                "required": ["prompt"]
            })),
            annotations: None,
            output_schema: None,
            icons: None,
            meta: None,
        }]
    }

    async fn handle_spawn_agent(
        &self,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> CallToolResult {
        let args: SpawnAgentArgs = match serde_json::from_value(serde_json::Value::Object(args)) {
            Ok(a) => a,
            Err(e) => {
                error!("spawn_agent: invalid arguments: {}", e);
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {}. Make sure the agent is enriching tool calls with context.",
                    e
                ))]);
            }
        };

        // Extract context (injected by agent)
        let ctx = &args.context;

        // Validate required context fields
        if !ctx.is_ready() {
            error!("spawn_agent: execution context incomplete: {:?}", ctx);
            return CallToolResult::error(vec![Content::text(
                "spawn_agent: execution context not ready. The agent should inject _context."
            )]);
        }

        let conversation_id = ConversationId::from_string(ctx.conversation_id.as_ref().unwrap());
        let user_id = UserId::from_string(ctx.user_id.as_ref().unwrap());
        let turn_id = TurnId::from_string(ctx.turn_id.as_ref().unwrap());
        let span_id = ctx.span_id.as_ref().map(|s| SpanId::from_string(s));
        let model_id = ctx.model_id.as_ref().unwrap();

        // Create model from ID
        let model = match create_model(model_id) {
            Ok(m) => m,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to create model '{}': {}",
                    model_id, e
                ))]);
            }
        };

        info!(
            "spawn_agent: creating subconversation from turn {} in conversation {}",
            turn_id.as_str(), conversation_id.as_str()
        );

        // 1. Create subconversation
        let sub_id = match self
            .inner
            .coordinator
            .spawn_subconversation(
                &conversation_id,
                &user_id,
                &turn_id,
                span_id.as_ref(),
                args.name.as_deref(),
            )
            .await
        {
            Ok(id) => id,
            Err(e) => {
                error!("Failed to spawn subconversation: {}", e);
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to spawn subconversation: {}",
                    e
                ))]);
            }
        };

        info!("spawn_agent: created subconversation {}", sub_id.as_str());

        // Build execution context for the subconversation's agent
        // (so nested spawn calls also get proper context)
        let sub_execution_context = ExecutionContext {
            user_id: Some(user_id.as_str().to_string()),
            conversation_id: Some(sub_id.as_str().to_string()),
            turn_id: Some(turn_id.as_str().to_string()), // Will be updated when subconversation creates its first turn
            span_id: span_id.as_ref().map(|s| s.as_str().to_string()),
            model_id: Some(model_id.clone()),
        };

        // 2. Run agent in subconversation
        if let Err(e) = self
            .inner
            .coordinator
            .run_agent_in_subconversation(
                &sub_id,
                args.prompt,
                args.system_prompt,
                model,
                Arc::clone(&self.inner.mcp_registry),
                Arc::clone(&self.inner.document_resolver),
                sub_execution_context,
            )
            .await
        {
            error!("Failed to run agent in subconversation: {}", e);
            return CallToolResult::error(vec![Content::text(format!(
                "Failed to run agent: {}",
                e
            ))]);
        }

        // 3. Get result
        let result = match self.inner.coordinator.get_subconversation_result(&sub_id).await {
            Ok(Some(r)) => r,
            Ok(None) => "(no result)".to_string(),
            Err(e) => {
                error!("Failed to get subconversation result: {}", e);
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to get result: {}",
                    e
                ))]);
            }
        };

        info!("spawn_agent: subconversation {} completed", sub_id.as_str());

        CallToolResult::success(vec![Content::text(format!(
            "Subconversation completed.\n\nSubconversation ID: {}\n\nResult:\n{}",
            sub_id.as_str(),
            result
        ))])
    }
}

impl ServerHandler for NoemaCoreServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Noema Core MCP server. Provides spawn_agent for creating subconversations."
                    .into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            let name = request.name.as_ref();
            let arguments = request.arguments.clone().unwrap_or_default();

            info!("noema-mcp-core: Calling tool: {}", name);

            match name {
                "spawn_agent" => Ok(self.handle_spawn_agent(arguments).await),
                _ => Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unknown tool: {}",
                    name
                ))])),
            }
        }
    }
}
