//! Integration tests for Session API with storage
//!
//! These tests verify that Session works correctly with the StorageCoordinator
//! and underlying memory stores.

use std::sync::Arc;

use llm::{ChatMessage, ChatPayload, ContentBlock, Role};

use crate::context::ConversationContext;
use crate::manager::CommitMode;
use crate::storage::coordinator::StorageCoordinator;
use crate::storage::ids::{ConversationId, UserId};
use crate::storage::implementations::memory::{
    MemoryAssetStore, MemoryBlobStore, MemoryEntityStore,
    MemoryStorage, MemoryTextStore, MemoryTurnStore,
};
use crate::storage::session::Session;

/// Create test coordinator with memory stores
fn make_test_coordinator() -> Arc<StorageCoordinator<MemoryStorage>> {
    let turn_store = Arc::new(MemoryTurnStore::new());
    let entity_store = Arc::new(MemoryEntityStore::new());

    Arc::new(StorageCoordinator::new(
        Arc::new(MemoryBlobStore::new()),
        Arc::new(MemoryAssetStore::new()),
        Arc::new(MemoryTextStore::new()),
        entity_store,
        turn_store,
    ))
}

/// Create a conversation using the coordinator
async fn create_test_conversation(
    coordinator: &StorageCoordinator<MemoryStorage>,
) -> ConversationId {
    let user_id = UserId::new();
    coordinator
        .create_conversation(&user_id, Some("Test Conversation"))
        .await
        .unwrap()
}

// ============================================================================
// Session Creation Tests
// ============================================================================

#[tokio::test]
async fn test_session_new_creates_empty_session() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    // New session has no messages
    assert_eq!(session.messages_for_display().len(), 0);
    assert_eq!(session.pending_messages().len(), 0);
    assert_eq!(session.len(), 0);
}

#[tokio::test]
async fn test_session_open_loads_existing_messages() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    // Create a session, add messages, and commit
    {
        let mut session = Session::<MemoryStorage>::new(
            coordinator.clone(),
            conversation_id.clone(),
        );

        // Add user message
        session.add(ChatMessage::user(ChatPayload::new(vec![
            ContentBlock::Text { text: "Hello, assistant!".to_string() },
        ])));

        // Add assistant message
        session.add(ChatMessage::assistant(ChatPayload::new(vec![
            ContentBlock::Text { text: "Hello, user!".to_string() },
        ])));

        // Commit with NewTurns mode (creates new turns)
        session.commit(Some("test-model"), &CommitMode::NewTurns).await.unwrap();
    }

    // Open a new session for the same conversation
    let opened_session = Session::<MemoryStorage>::open(
        coordinator.clone(),
        conversation_id,
    ).await.unwrap();

    // Should have the committed messages
    assert_eq!(opened_session.messages_for_display().len(), 2);
    assert_eq!(opened_session.pending_messages().len(), 0);
}

// ============================================================================
// Message Management Tests
// ============================================================================

#[tokio::test]
async fn test_session_add_to_pending() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    // Add messages - they go to pending
    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "First message".to_string() },
    ])));
    session.add(ChatMessage::assistant(ChatPayload::new(vec![
        ContentBlock::Text { text: "Second message".to_string() },
    ])));

    // Pending has 2, resolved has 0
    assert_eq!(session.pending_messages().len(), 2);
    assert_eq!(session.messages_for_display().len(), 0);
    assert_eq!(session.len(), 2);
}

#[tokio::test]
async fn test_session_commit_moves_to_resolved() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "Hello".to_string() },
    ])));

    // Before commit
    assert_eq!(session.pending_messages().len(), 1);
    assert_eq!(session.messages_for_display().len(), 0);

    // Commit
    session.commit(Some("test-model"), &CommitMode::NewTurns).await.unwrap();

    // After commit: pending empty, resolved has message
    assert_eq!(session.pending_messages().len(), 0);
    assert_eq!(session.messages_for_display().len(), 1);

    let msg = &session.messages_for_display()[0];
    assert_eq!(msg.role, Role::User);
}

#[tokio::test]
async fn test_session_all_messages_combines_resolved_and_pending() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    // Add and commit first message
    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "Committed".to_string() },
    ])));
    session.commit(None, &CommitMode::NewTurns).await.unwrap();

    // Add second message without committing
    session.add(ChatMessage::assistant(ChatPayload::new(vec![
        ContentBlock::Text { text: "Pending".to_string() },
    ])));

    // all_messages() includes both
    let all = session.all_messages();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].role, Role::User);
    assert_eq!(all[1].role, Role::Assistant);
}

// ============================================================================
// Context Interface Tests (ConversationContext trait)
// ============================================================================

#[tokio::test]
async fn test_session_as_conversation_context() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    // Use ConversationContext trait methods
    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "Hello via trait".to_string() },
    ])));

    // len() should count pending
    assert_eq!(session.len(), 1);
    assert!(!session.is_empty());

    // pending() should return pending messages
    assert_eq!(session.pending().len(), 1);

    // messages() returns for LLM (includes pending)
    let guard = session.messages().await.unwrap();
    assert_eq!(guard.len(), 1);
}

#[tokio::test]
async fn test_session_context_commit() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "Test".to_string() },
    ])));

    // Use trait commit (uses default CommitMode::NewTurns)
    ConversationContext::commit(&mut session).await.unwrap();

    assert_eq!(session.pending().len(), 0);
    assert_eq!(session.messages_for_display().len(), 1);
}

// ============================================================================
// Truncation Tests
// ============================================================================

#[tokio::test]
async fn test_session_truncate_clears_all() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    // Add and commit messages
    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "User message".to_string() },
    ])));
    session.add(ChatMessage::assistant(ChatPayload::new(vec![
        ContentBlock::Text { text: "Assistant message".to_string() },
    ])));
    session.commit(None, &CommitMode::NewTurns).await.unwrap();

    assert_eq!(session.messages_for_display().len(), 2);

    // Truncate all (None)
    session.truncate(None);

    // Should clear resolved cache
    assert_eq!(session.messages_for_display().len(), 0);
    assert_eq!(session.pending_messages().len(), 0);
}

#[tokio::test]
async fn test_session_truncate_at_turn() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    // Add and commit multiple turns
    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "First user".to_string() },
    ])));
    session.commit(None, &CommitMode::NewTurns).await.unwrap();

    session.add(ChatMessage::assistant(ChatPayload::new(vec![
        ContentBlock::Text { text: "First assistant".to_string() },
    ])));
    session.commit(None, &CommitMode::NewTurns).await.unwrap();

    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "Second user".to_string() },
    ])));
    session.commit(None, &CommitMode::NewTurns).await.unwrap();

    assert_eq!(session.messages_for_display().len(), 3);

    // Get the turn_id of the second message (assistant)
    let second_turn_id = session.messages_for_display()[1].turn_id.clone();

    // Truncate to before second turn (keeps only first message)
    session.truncate(Some(&second_turn_id));

    assert_eq!(session.messages_for_display().len(), 1);
    assert_eq!(session.messages_for_display()[0].role, Role::User);
}

// ============================================================================
// Cache Management Tests
// ============================================================================

#[tokio::test]
async fn test_session_clear_cache() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    // Add and commit
    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "Hello".to_string() },
    ])));
    session.commit(None, &CommitMode::NewTurns).await.unwrap();

    assert_eq!(session.messages_for_display().len(), 1);

    // Clear cache
    session.clear_cache();

    // Resolved cache is cleared
    assert_eq!(session.messages_for_display().len(), 0);
}

#[tokio::test]
async fn test_session_clear_pending() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    // Add without committing
    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "Pending message".to_string() },
    ])));

    assert_eq!(session.pending_messages().len(), 1);

    // Clear pending
    session.clear_pending();

    assert_eq!(session.pending_messages().len(), 0);
}

// ============================================================================
// CommitMode Tests
// ============================================================================

#[tokio::test]
async fn test_session_commit_at_turn_regeneration() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator.clone(),
        conversation_id.clone(),
    );

    // Add user message and first assistant response
    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "What is 2+2?".to_string() },
    ])));
    session.commit(Some("model-v1"), &CommitMode::NewTurns).await.unwrap();

    session.add(ChatMessage::assistant(ChatPayload::new(vec![
        ContentBlock::Text { text: "2+2 equals 4".to_string() },
    ])));
    session.commit(Some("model-v1"), &CommitMode::NewTurns).await.unwrap();

    assert_eq!(session.messages_for_display().len(), 2);

    // Get the assistant turn_id for regeneration
    let assistant_turn_id = session.messages_for_display()[1].turn_id.clone();

    // Truncate to before assistant turn
    session.truncate(Some(&assistant_turn_id));
    assert_eq!(session.messages_for_display().len(), 1);

    // Add new assistant response and commit at the same turn (regeneration)
    session.add(ChatMessage::assistant(ChatPayload::new(vec![
        ContentBlock::Text { text: "The answer is 4!".to_string() },
    ])));
    session.commit(Some("model-v2"), &CommitMode::AtTurn(assistant_turn_id)).await.unwrap();

    // Should still have 2 messages
    assert_eq!(session.messages_for_display().len(), 2);
}

// ============================================================================
// Message Content Tests
// ============================================================================

#[tokio::test]
async fn test_session_preserves_message_content() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    // Add message with specific content
    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "Hello, world!".to_string() },
    ])));
    session.commit(None, &CommitMode::NewTurns).await.unwrap();

    // Verify content is preserved
    let resolved = &session.messages_for_display()[0];
    assert_eq!(resolved.content.len(), 1);

    match &resolved.content[0] {
        crate::storage::session::ResolvedContent::Text { text } => {
            assert_eq!(text, "Hello, world!");
        }
        other => panic!("Expected Text, got {:?}", other),
    }
}

#[tokio::test]
async fn test_session_multi_content_message() {
    let coordinator = make_test_coordinator();
    let conversation_id = create_test_conversation(&coordinator).await;

    let mut session = Session::<MemoryStorage>::new(
        coordinator,
        conversation_id,
    );

    // Add message with multiple content blocks
    session.add(ChatMessage::user(ChatPayload::new(vec![
        ContentBlock::Text { text: "Part 1".to_string() },
        ContentBlock::Text { text: "Part 2".to_string() },
    ])));
    session.commit(None, &CommitMode::NewTurns).await.unwrap();

    let resolved = &session.messages_for_display()[0];
    assert_eq!(resolved.content.len(), 2);
}
