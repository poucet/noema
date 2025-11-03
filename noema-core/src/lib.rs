//! Core traits and implementations for the noema agent framework
//!
//! This crate provides:
//! - **Traits**: `ConversationContext`, `Agent`, `Transaction`
//! - **Implementations**: `SimpleAgent`, `ToolAgent`
//!
//! # Example
//!
//! ```ignore
//! use noema_core::{Agent, SimpleAgent};
//!
//! let agent = SimpleAgent::new();
//! let messages = agent.execute(&context, &model).await?;
//! ```
pub mod agent;
pub mod agents;
pub mod context;
pub mod session;
pub mod transaction;

pub use agent::Agent;
pub use agents::{SimpleAgent, ToolAgent};
pub use context::ConversationContext;
pub use session::{Session, SimpleContext, TransactionContext};
pub use transaction::Transaction;
