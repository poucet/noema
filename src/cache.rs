use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::completion::{AsyncCompleter, Completion, CompletionContext};
use crate::error::CompletionError;

/// Wrapper that adds caching to any AsyncCompleter
pub struct CachedCompleter<T: AsyncCompleter> {
    inner: T,
    cache: Arc<RwLock<HashMap<String, CachedEntry<T::Metadata>>>>,
    ttl: Duration,
}

struct CachedEntry<M: Serialize> {
    completions: Vec<Completion<M>>,
    timestamp: Instant,
}

impl<T: AsyncCompleter> CachedCompleter<T> {
    /// Create a new cached completer with the given TTL
    pub fn new(inner: T, ttl: Duration) -> Self {
        Self {
            inner,
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    /// Create a cached completer with default 5 minute TTL
    pub fn with_default_ttl(inner: T) -> Self {
        Self::new(inner, Duration::from_secs(300))
    }

    /// Clear the cache
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }

    /// Create cache key from partial input and context
    fn cache_key(partial: &str, context: &CompletionContext) -> String {
        format!("{}:{}", context.input, partial)
    }
}

#[async_trait]
impl<T: AsyncCompleter> AsyncCompleter for CachedCompleter<T> {
    type Metadata = T::Metadata;

    async fn complete(
        &self,
        partial: &str,
        context: &CompletionContext,
    ) -> Result<Vec<Completion<Self::Metadata>>, CompletionError> {
        let cache_key = Self::cache_key(partial, context);

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(&cache_key) {
                if entry.timestamp.elapsed() < self.ttl {
                    return Ok(entry.completions.clone());
                }
            }
        }

        // Cache miss or expired - fetch fresh
        let completions = self.inner.complete(partial, context).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                cache_key,
                CachedEntry {
                    completions: completions.clone(),
                    timestamp: Instant::now(),
                },
            );
        }

        Ok(completions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockCompleter;

    #[async_trait]
    impl AsyncCompleter for MockCompleter {
        type Metadata = ();

        async fn complete(
            &self,
            partial: &str,
            _context: &CompletionContext,
        ) -> Result<Vec<Completion<()>>, CompletionError> {
            Ok(vec![Completion::simple(format!("completion-{}", partial))])
        }
    }

    #[tokio::test]
    async fn test_caching() {
        let completer = CachedCompleter::new(MockCompleter, Duration::from_secs(1));
        let ctx = CompletionContext::new("/test".to_string(), 5);

        // First call - should hit inner completer
        let result1 = completer.complete("foo", &ctx).await.unwrap();
        assert_eq!(result1.len(), 1);
        assert_eq!(result1[0].value, "completion-foo");

        // Second call - should return cached result
        let result2 = completer.complete("foo", &ctx).await.unwrap();
        assert_eq!(result2, result1);

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // Third call - should fetch fresh after expiry
        let result3 = completer.complete("foo", &ctx).await.unwrap();
        assert_eq!(result3.len(), 1);
    }
}
