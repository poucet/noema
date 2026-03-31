# noema-core

Core trait definitions for the noema agent framework.

## Overview

This crate provides the foundational abstractions for building composable conversational agents:

- **`ConversationContext`** - Read-only view of conversation history
- **`Agent`** - Interface for agents that process messages and produce responses
- **`Transaction`** - Write buffer for uncommitted messages with rollback support

## Design Philosophy

1. **Separation of concerns**: Context (read) and Transaction (write) are separate
2. **Composability**: Agents can be stacked and composed
3. **Transactional semantics**: All-or-nothing commits with explicit control
4. **Storage agnostic**: No persistence logic, just traits

## Core Concepts

### ConversationContext

Provides read-only access to conversation messages via an iterator interface:

```rust
pub trait ConversationContext {
    fn iter(&self) -> impl Iterator<Item = &ChatMessage>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}
```

**Design:** Intentionally synchronous and simple. All messages must be in-memory.

Different implementations can provide:
- In-memory slices
- Windowed views (last N messages)
- Filtered contexts (by role, content, etc.)
- Combined contexts (system prompt + conversation)

**Note:** For "lazy loading", load messages at the session layer before creating the context. This keeps the trait simple and easier to implement.

### Agent

Examines context and produces messages:

```rust
#[async_trait]
pub trait Agent {
    async fn execute(
        &self,
        context: &impl ConversationContext,
        model: &impl ChatModel,
    ) -> Result<Vec<ChatMessage>>;

    async fn execute_stream(
        &self,
        context: &impl ConversationContext,
        model: &impl ChatModel,
    ) -> Result<Pin<Box<dyn Stream<Item = ChatMessage> + Send>>>;
}
```

Key design decisions:
- **No separate input parameter** - User input (or tool results, etc.) is already in the context
- **Pure functions** - No side effects on context
- **Returns all messages produced** - Typically assistant responses, but can include tool calls, etc.
- **Composable** - Can be stacked into pipelines
- **Flexible triggering** - Can be triggered by user input, tool results, or anything in context

The session/transaction layer is responsible for adding user input to the context before calling the agent.

### Transaction

Buffer for uncommitted messages:

```rust
pub struct Transaction {
    // pending messages
}

impl Transaction {
    pub fn new() -> Self;
    pub fn add(&mut self, message: ChatMessage);
    pub fn extend(&mut self, messages: impl IntoIterator<Item = ChatMessage>);
    pub fn pending(&self) -> &[ChatMessage];
    pub fn commit(self) -> Vec<ChatMessage>;
    pub fn rollback(self);
}
```

Transactions provide:
- Validation before commit
- Rollback on error
- All-or-nothing semantics
- Warning on accidental drop

## Usage Pattern

```rust
// 1. Begin transaction
let mut tx = Transaction::new();

// 2. Add user input to transaction
tx.add(ChatMessage::user("Hello".into()));

// 3. Create context that includes committed + pending messages
let context = session.transaction_context(&tx);

// 4. Execute agent (agent sees user input in context)
let messages = agent.execute(&context, &model).await?;
tx.extend(messages);

// 5. Inspect before committing
if is_valid(tx.pending()) {
    session.commit(tx);  // Persist to storage
} else {
    tx.rollback();  // Discard everything
}
```

### Convenience Method

For simple cases, the session layer provides a convenience method:

```rust
// This handles: add user input → execute → commit
let messages = session.send(&agent, &model, "Hello".into()).await?;
```

## Why Separate Traits?

This design allows:

1. **Testing**: Mock contexts without storage
2. **Performance**: Optimize context implementations independently
3. **Flexibility**: Different storage backends without changing agent code
4. **Safety**: Can't accidentally mutate conversation history
5. **Composition**: Build complex contexts from simple ones

## License

[Your license here]
