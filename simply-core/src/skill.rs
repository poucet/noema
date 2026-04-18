//! Skill system — pluggable capabilities that extend the daemon with tools.
//!
//! A skill is a self-contained module that provides tool definitions and
//! execution logic. Skills get access to daemon services (document store,
//! asset storage, etc.) via [`SkillContext`], but are built as separate
//! crates with no dependency on the daemon itself.
//!
//! Current registration is explicit (daemon constructs skills at startup).
//! Future: dynamic loading via shared libraries.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};

use crate::storage::coordinator::StorageCoordinator;
use crate::storage::ids::UserId;
use crate::storage::traits::{DocumentStore, StorageTypes, Stores};

/// Services available to skills. Passed to [`Skill::init`] at startup.
///
/// Skills should store what they need from this context and release the rest.
/// The context is generic over storage types to avoid boxing — skills that
/// need storage access should be generic too, or use the trait-object accessors.
pub struct SkillContext<S: StorageTypes> {
    pub stores: Arc<dyn Stores<S>>,
    pub coordinator: Arc<StorageCoordinator<S>>,
}

impl<S: StorageTypes> Clone for SkillContext<S> {
    fn clone(&self) -> Self {
        Self {
            stores: Arc::clone(&self.stores),
            coordinator: Arc::clone(&self.coordinator),
        }
    }
}

/// A pluggable skill that provides tools to the daemon.
///
/// Skills are constructed by the daemon at startup and registered
/// with the tool service. Each skill can provide multiple tools.
///
/// # Lifecycle
/// 1. Daemon constructs the skill with a [`SkillContext`]
/// 2. `name()` and `tools()` are called to register with the tool service
/// 3. `call_tool()` is called when an agent or user invokes a tool
///
/// # Future: dynamic loading
/// The current design uses explicit registration. To support dynamic loading:
/// - Skills would be compiled as cdylib crates
/// - The daemon would load `.so`/`.dylib` files from a skills directory
/// - Each library would export a `create_skill(SkillContext) -> Box<dyn Skill>` function
/// - The `Skill` trait is already object-safe for this purpose
#[async_trait]
pub trait Skill: Send + Sync {
    /// Unique name for this skill (e.g., "gdocs", "github").
    fn name(&self) -> &str;

    /// Tool definitions provided by this skill.
    fn tools(&self) -> Vec<ToolDefinition>;

    /// Execute a tool by name.
    ///
    /// `user_id` identifies the calling user (for scoped storage access).
    /// `arguments` is the JSON arguments from the tool call.
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
        user_id: &UserId,
    ) -> Result<Vec<ToolResultContent>>;
}
