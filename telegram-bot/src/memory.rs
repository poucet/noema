//! Per-chat conversation memory for Telegram.
//!
//! Telegram's Bot API does not expose arbitrary chat history to bots, so the
//! bridge keeps a bounded in-process transcript and seeds each daemon session
//! from that transcript.

use std::collections::HashMap;
use std::sync::Arc;

use simply_daemon_api::SeedMessage;
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::api::Message;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConversationKey {
    pub chat_id: i64,
    pub thread_id: Option<i64>,
}

impl ConversationKey {
    pub fn from_message(message: &Message) -> Self {
        Self {
            chat_id: message.chat.id,
            thread_id: message.message_thread_id,
        }
    }
}

#[derive(Clone, Default)]
pub struct ConversationMemory {
    turns: Arc<Mutex<HashMap<ConversationKey, Vec<SeedMessage>>>>,
    locks: Arc<Mutex<HashMap<ConversationKey, Arc<Mutex<()>>>>>,
}

impl ConversationMemory {
    pub async fn lock(&self, key: ConversationKey) -> OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.locks.lock().await;
            locks
                .entry(key)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        lock.lock_owned().await
    }

    pub async fn seed(&self, key: ConversationKey, limit: usize) -> Vec<SeedMessage> {
        let turns = self.turns.lock().await;
        let Some(history) = turns.get(&key) else {
            return Vec::new();
        };
        let start = history.len().saturating_sub(limit);
        history[start..].to_vec()
    }

    pub async fn append(
        &self,
        key: ConversationKey,
        mut new_turns: Vec<SeedMessage>,
        limit: usize,
    ) {
        if new_turns.is_empty() {
            return;
        }

        let mut turns = self.turns.lock().await;
        let history = turns.entry(key).or_default();
        history.append(&mut new_turns);

        if history.len() > limit {
            let overflow = history.len() - limit;
            history.drain(0..overflow);
        }
    }

    pub async fn clear(&self, key: ConversationKey) {
        self.turns.lock().await.remove(&key);
    }
}
