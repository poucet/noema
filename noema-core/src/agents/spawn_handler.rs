//! SpawnHandler - Handles spawning subconversations from agent tool calls
//!
//! When an LLM calls the `spawn_agent` tool, the SpawnHandler:
//! 1. Creates a subconversation linked to the parent
//! 2. Runs an agent in the subconversation with the given prompt
//! 3. Returns the result to the parent conversation

use anyhow::Result;
use async_trait::async_trait;
use llm::ToolResultContent;

use crate::context::ConversationContext;

/// Arguments for spawning a subconversation agent
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct SpawnAgentArgs {
    /// The task/prompt for the spawned agent
    pub prompt: String,
    /// Optional system prompt for the spawned agent
    pub system_prompt: Option<String>,
    /// Optional name for the subconversation
    pub name: Option<String>,
}

/// Result from a spawned subconversation
#[derive(Debug, Clone)]
pub struct SpawnResult {
    /// The subconversation ID (entity ID)
    pub subconversation_id: String,
    /// The result text from the subconversation
    pub result: String,
}

/// Trait for handling spawn_agent tool calls.
///
/// Implementors create and run subconversations, returning the result.
/// This is called by McpAgent when it encounters a spawn_agent tool call.
#[async_trait]
pub trait SpawnHandler: Send + Sync {
    /// Spawn a subconversation and run an agent in it.
    ///
    /// # Arguments
    /// * `parent_turn_id` - The turn in the parent where spawn was triggered
    /// * `parent_span_id` - The span in the parent (optional)
    /// * `args` - The spawn arguments (prompt, system_prompt, name)
    /// * `model` - The model to use for the spawned agent
    ///
    /// # Returns
    /// The spawn result containing subconversation_id and result text
    async fn spawn(
        &self,
        parent_turn_id: &str,
        parent_span_id: Option<&str>,
        args: SpawnAgentArgs,
        model: Arc<dyn llm::ChatModel + Send + Sync>,
    ) -> Result<SpawnResult>;
}

/// Convert SpawnResult to tool result content
impl SpawnResult {
    pub fn to_tool_result_content(&self) -> Vec<ToolResultContent> {
        vec![ToolResultContent::text(&format!(
            "Subconversation completed.\n\nSubconversation ID: {}\n\nResult:\n{}",
            self.subconversation_id, self.result
        ))]
    }
}

/// Tool definition for spawn_agent
pub fn spawn_agent_tool_definition() -> llm::ToolDefinition {
    llm::ToolDefinition {
        name: "spawn_agent".to_string(),
        description: Some(
            "Spawn a subconversation to handle a complex subtask. The spawned agent runs \
             independently and returns its result. Use this for tasks that require focused \
             attention or multiple tool calls that are separate from the main conversation flow."
                .to_string(),
        ),
        input_schema: schemars::schema_for!(SpawnAgentArgs),
    }
}

// ============================================================================
// Concrete SpawnHandler Implementation
// ============================================================================

use crate::mcp::{McpRegistry, McpToolRegistry};
use crate::storage::coordinator::StorageCoordinator;
use crate::storage::document_resolver::DocumentResolver;
use crate::storage::ids::{SpanId, TurnId, UserId};
use crate::storage::session::Session;
use crate::storage::traits::StorageTypes;
use crate::Agent;
use llm::{ChatMessage, ChatPayload, ContentBlock};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Concrete SpawnHandler that creates and runs subconversations.
///
/// This handler creates a new conversation linked to the parent,
/// runs a simplified agent (with MCP tools but no spawn capability
/// to prevent infinite recursion), and returns the result.
pub struct ConversationSpawnHandler<S: StorageTypes> {
    coordinator: Arc<StorageCoordinator<S>>,
    user_id: UserId,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    document_resolver: Arc<dyn DocumentResolver>,
    parent_conversation_id: String,
}

impl<S: StorageTypes> ConversationSpawnHandler<S> {
    pub fn new(
        coordinator: Arc<StorageCoordinator<S>>,
        user_id: UserId,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
        parent_conversation_id: String,
    ) -> Self {
        Self {
            coordinator,
            user_id,
            mcp_registry,
            document_resolver,
            parent_conversation_id,
        }
    }
}

#[async_trait]
impl<S: StorageTypes> SpawnHandler for ConversationSpawnHandler<S> {
    async fn spawn(
        &self,
        parent_turn_id: &str,
        parent_span_id: Option<&str>,
        args: SpawnAgentArgs,
        model: Arc<dyn llm::ChatModel + Send + Sync>,
    ) -> Result<SpawnResult> {
        use crate::agents::McpAgent;
        use crate::storage::ids::ConversationId;

        let parent_conv_id = ConversationId::from_string(&self.parent_conversation_id);
        let turn_id = TurnId::from_string(parent_turn_id);
        let span_id = parent_span_id.map(SpanId::from_string);

        // 1. Create subconversation linked to parent
        let sub_id = self
            .coordinator
            .spawn_subconversation(
                &parent_conv_id,
                &self.user_id,
                &turn_id,
                span_id.as_ref(),
                args.name.as_deref(),
            )
            .await?;

        // 2. Open a session for the subconversation
        let resolved_messages = self.coordinator.open_session(&sub_id).await?;
        let mut session = Session::new(self.coordinator.clone(), sub_id.clone());
        for msg in resolved_messages {
            session.add_resolved(msg);
        }

        // 3. Add system prompt first if provided
        if let Some(system) = &args.system_prompt {
            let system_message = ChatMessage::system(ChatPayload::new(vec![ContentBlock::Text {
                text: system.clone(),
            }]));
            session.add(system_message);
        }

        // 4. Add the prompt as a user message
        let user_message = ChatMessage::user(ChatPayload::new(vec![ContentBlock::Text {
            text: args.prompt.clone(),
        }]));
        session.add(user_message);

        // 5. Create agent WITHOUT spawn handler (prevent recursion)
        let tool_registry = McpToolRegistry::new(Arc::clone(&self.mcp_registry));
        let agent = McpAgent::new(
            Arc::new(tool_registry),
            5, // Lower max iterations for subconversations
            Arc::clone(&self.document_resolver),
        );

        // 6. Run agent
        agent.execute(&mut session, model).await?;

        // 7. Get result from subconversation
        let result = self
            .coordinator
            .get_subconversation_result(&sub_id)
            .await?
            .unwrap_or_else(|| "(no result)".to_string());

        Ok(SpawnResult {
            subconversation_id: sub_id.as_str().to_string(),
            result,
        })
    }
}
