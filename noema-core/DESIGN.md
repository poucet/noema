# noema-core Design Document

## Overview

`noema-core` defines the foundational trait abstractions for building composable conversational agents. It contains **only trait definitions** - no implementations, no storage, no concrete types.

## Core Traits

### 1. ConversationContext (Read-Only)

```rust
pub trait ConversationContext {
    fn iter(&self) -> impl Iterator<Item = &ChatMessage>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}
```

**Purpose:** Provide read-only access to conversation messages.

**Key Design Decisions:**
- Iterator-based for zero-copy iteration
- Synchronous (no async) - keeps trait simple
- All messages must be in-memory (no lazy loading in the trait)
- Can be implemented by: slices, vectors, windowed views, filtered contexts, combined contexts
- Immutable - agents can't modify history
- No `to_vec()` convenience method - just use `context.iter().cloned().collect()`

**Why no lazy loading?**
- Would require `async fn` or `&mut self` for caching
- Keeps trait simple and easier to implement
- Session layer can load messages before creating context
- For large conversations, use windowed or filtered contexts

### 2. Agent (Pure Function)

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

**Purpose:** Transform context into new messages.

**Key Design Decisions:**
- No separate `input` parameter - input is already in the context
- Pure function - no side effects
- Returns all messages produced (for multi-turn agents)
- Streaming support for real-time UI updates

**Why no input parameter?**
- More flexible: can be triggered by user input, tool results, system messages, or "continue from here"
- Simpler composition: when stacking agents, output of agent1 naturally becomes input to agent2
- Clearer separation: session layer adds user input, agent just processes context

### 3. Transaction (Write Buffer)

```rust
pub struct Transaction {
    pending: Vec<ChatMessage>,
    committed: bool,
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

**Purpose:** Buffer for uncommitted messages with transactional semantics.

**Key Design Decisions:**
- Consumes itself on commit/rollback (can't reuse)
- Warns on drop without finalization
- Simple Vec-backed implementation
- All-or-nothing semantics

## Architecture Layers

```
┌─────────────────────────────────────┐
│  UI Layer (CLI, TUI)                │
│  - Display messages                 │
│  - Handle user input                │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│  Session Layer (not in noema-core)  │
│  - Manages conversation state       │
│  - Creates contexts & transactions  │
│  - Commits to storage              │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│  Agent Layer (implements Agent)     │
│  - SimpleAgent                      │
│  - ToolAgent                        │
│  - Custom agents                    │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│  noema-core (THIS CRATE)            │
│  - ConversationContext trait        │
│  - Agent trait                      │
│  - Transaction struct               │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│  LLM Layer                          │
│  - ChatModel trait                  │
│  - Provider implementations         │
└─────────────────────────────────────┘
```

## Typical Flow

```rust
// 1. Session adds user input to transaction
let mut tx = Transaction::new();
tx.add(ChatMessage::user("Hello"));

// 2. Session creates context (committed + pending)
let context = session.transaction_context(&tx);

// 3. Agent executes (sees user input in context)
let messages = agent.execute(&context, &model).await?;

// 4. Session adds agent's output to transaction
tx.extend(messages);

// 5. Session validates and commits (or rolls back)
if is_valid(tx.pending()) {
    session.commit(tx);  // Persists to storage
} else {
    tx.rollback();  // Discards everything
}
```

## Benefits of This Design

### 1. Composability
Agents can be stacked:
```rust
let pipeline = AgentPipeline::new()
    .add(SearchAgent)
    .add(SummarizeAgent)
    .add(TranslateAgent);

let messages = pipeline.execute(&context, &model).await?;
```

### 2. Testability
```rust
// Mock context for testing
struct MockContext(Vec<ChatMessage>);
impl ConversationContext for MockContext { /* ... */ }

let context = MockContext(vec![
    ChatMessage::user("test input")
]);
let messages = agent.execute(&context, &mock_model).await?;
```

### 3. Flexibility
```rust
// Different contexts for different needs
let windowed = WindowedContext::new(messages, 10);  // Last 10 only
let filtered = FilteredContext::without_system(messages);  // No system msgs
let composed = ComposedContext::new()
    .with_system("You are helpful")
    .with_messages(messages);
```

### 4. Storage Agnostic
- Transaction doesn't know about storage
- Agent doesn't know about storage
- Session layer handles persistence

### 5. Safe
- Context is read-only (agents can't mutate history)
- Transactions are explicit (can't accidentally commit)
- Drop guard warns about lost messages

## What's NOT in This Crate

- ❌ Concrete Agent implementations
- ❌ Concrete Context implementations (besides basic requirements)
- ❌ Storage layer
- ❌ Session management
- ❌ Message types (comes from `llm` crate)
- ❌ Model implementations

This crate is **pure abstractions** - just traits and the Transaction type.

## Dependencies

Minimal dependencies by design:
- `anyhow` - Error handling
- `async-trait` - Async trait support
- `futures` - Stream types
- `llm` - Message and model types (internal dependency)

No database, no HTTP, no serialization, no UI dependencies.
