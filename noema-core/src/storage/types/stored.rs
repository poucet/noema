//! Generic stored wrapper types
//!
//! Provides common patterns for database-stored entities:
//! - `Stored<Id, T>` - Immutable entities with ID, content, and created_at
//! - `Editable<T>` - Wrapper that adds updated_at to mutable entities
//!
//! Use `Stored<Id, Editable<T>>` for entities that can be modified after creation.

use std::ops::Deref;

/// A stored entity wrapper that adds ID and timestamp to any content type.
///
/// This generic wrapper captures the common pattern where stored entities
/// consist of an ID, the actual data, and a creation timestamp.
///
/// Implements `Deref` to allow transparent access to the inner content.
#[derive(Clone, Debug)]
pub struct Stored<Id, T> {
    /// Unique identifier
    pub id: Id,
    /// The stored content
    pub content: T,
    /// Unix timestamp (milliseconds) when created
    pub created_at: i64,
}

impl<Id, T> Stored<Id, T> {
    /// Create a new stored entity
    pub fn new(id: Id, content: T, created_at: i64) -> Self {
        Self {
            id,
            content,
            created_at,
        }
    }

    /// Get a reference to the ID
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// Get the creation timestamp
    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    /// Consume and return the inner content
    pub fn into_content(self) -> T {
        self.content
    }

    /// Map the content to a new type
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Stored<Id, U> {
        Stored {
            id: self.id,
            content: f(self.content),
            created_at: self.created_at,
        }
    }
}

impl<Id, T> Deref for Stored<Id, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.content
    }
}

impl<Id: Clone, T> Stored<Id, T> {
    /// Clone just the ID
    pub fn clone_id(&self) -> Id {
        self.id.clone()
    }
}

// ============================================================================
// Editable wrapper
// ============================================================================

/// Wrapper for entities that can be modified after creation.
///
/// Adds `updated_at` timestamp tracking. Use with `Stored<Id, Editable<T>>`
/// for the full pattern:
///
/// ```ignore
/// // A document that can be edited
/// type StoredDocument = Stored<DocumentId, Editable<Document>>;
/// ```
#[derive(Clone, Debug)]
pub struct Editable<T> {
    /// The content
    pub content: T,
    /// Unix timestamp (milliseconds) when last updated
    pub updated_at: i64,
}

impl<T> Editable<T> {
    /// Create a new editable wrapper
    pub fn new(content: T, updated_at: i64) -> Self {
        Self { content, updated_at }
    }

    /// Get the last update timestamp
    pub fn updated_at(&self) -> i64 {
        self.updated_at
    }

    /// Consume and return the inner content
    pub fn into_content(self) -> T {
        self.content
    }

    /// Map the content to a new type
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Editable<U> {
        Editable {
            content: f(self.content),
            updated_at: self.updated_at,
        }
    }
}

impl<T> Deref for Editable<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct TestId(String);

    #[derive(Clone, Debug, PartialEq)]
    struct TestContent {
        name: String,
        value: i32,
    }

    #[test]
    fn test_stored_deref() {
        let stored = Stored::new(
            TestId("test-1".to_string()),
            TestContent {
                name: "foo".to_string(),
                value: 42,
            },
            1000,
        );

        // Deref allows direct access to content fields
        assert_eq!(stored.name, "foo");
        assert_eq!(stored.value, 42);
    }

    #[test]
    fn test_stored_accessors() {
        let stored = Stored::new(
            TestId("test-2".to_string()),
            TestContent {
                name: "bar".to_string(),
                value: 100,
            },
            2000,
        );

        assert_eq!(stored.id().0, "test-2");
        assert_eq!(stored.created_at(), 2000);
    }

    #[test]
    fn test_into_content() {
        let stored = Stored::new(
            TestId("test-3".to_string()),
            TestContent {
                name: "baz".to_string(),
                value: 200,
            },
            3000,
        );

        let content = stored.into_content();
        assert_eq!(content.name, "baz");
        assert_eq!(content.value, 200);
    }

    #[test]
    fn test_map() {
        let stored = Stored::new(TestId("test-4".to_string()), 10i32, 4000);

        let mapped = stored.map(|v| v * 2);
        assert_eq!(*mapped, 20);
        assert_eq!(mapped.id().0, "test-4");
    }
}
