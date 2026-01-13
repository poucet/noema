//! Generic stored wrapper types
//!
//! Provides composable patterns for database-stored entities:
//!
//! - `Keyed<Id, T>` - Adds an ID to any type
//! - `Timestamped<T>` - Adds created_at timestamp to any type
//! - `Editable<T>` - Adds updated_at timestamp for mutable entities
//!
//! These can be composed:
//! - `Keyed<Id, Timestamped<T>>` - Entity with ID and creation timestamp
//! - `Keyed<Id, Timestamped<Editable<T>>>` - Entity that can be modified after creation

use std::ops::{Deref, DerefMut};

// ============================================================================
// Keyed - adds an ID to any type
// ============================================================================

/// A wrapper that adds an ID to any type.
///
/// Implements `Deref` to allow transparent access to the inner content.
///
/// # Examples
///
/// ```ignore
/// // Basic usage
/// let keyed = Keyed::new(UserId::new(), user_data);
/// println!("User: {}", keyed.name); // Deref to inner
///
/// // With Stored for timestamped data
/// type StoredUser = Keyed<UserId, Stored<User>>;
/// ```
#[derive(Clone, Debug)]
pub struct Keyed<Id, T> {
    /// Unique identifier
    pub id: Id,
    /// The content
    pub content: T,
}

impl<Id, T> Keyed<Id, T> {
    /// Create a new keyed entity
    pub fn new(id: Id, content: T) -> Self {
        Self { id, content }
    }

    /// Get a reference to the ID
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// Consume and return the inner content
    pub fn into_content(self) -> T {
        self.content
    }

    /// Map the content to a new type
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Keyed<Id, U> {
        Keyed {
            id: self.id,
            content: f(self.content),
        }
    }
}

impl<Id, T> Deref for Keyed<Id, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.content
    }
}

impl<Id, T> DerefMut for Keyed<Id, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.content
    }
}

// ============================================================================
// Timestamped - adds created_at timestamp
// ============================================================================

/// A wrapper that adds a creation timestamp to any type.
///
/// Implements `Deref` to allow transparent access to the inner content.
///
/// # Examples
///
/// ```ignore
/// // Basic usage
/// let timestamped = Timestamped::new(data, unix_timestamp());
///
/// // Commonly composed with Keyed
/// type StoredMessage = Keyed<MessageId, Timestamped<Message>>;
/// ```
#[derive(Clone, Debug)]
pub struct Timestamped<T> {
    /// The content
    pub content: T,
    /// Unix timestamp (milliseconds) when created
    pub created_at: i64,
}

impl<T> Timestamped<T> {
    /// Create a new timestamped entity
    pub fn new(content: T, created_at: i64) -> Self {
        Self { content, created_at }
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
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Timestamped<U> {
        Timestamped {
            content: f(self.content),
            created_at: self.created_at,
        }
    }
}

impl<T> Deref for Timestamped<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.content
    }
}

impl<T> DerefMut for Timestamped<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.content
    }
}

// ============================================================================
// Editable - adds updated_at timestamp for mutable entities
// ============================================================================

/// Wrapper for entities that can be modified after creation.
///
/// Adds `updated_at` timestamp tracking.
///
/// # Examples
///
/// ```ignore
/// // For documents that can be edited
/// type StoredDocument = Keyed<DocumentId, Stored<Editable<Document>>>;
/// ```
#[derive(Clone, Debug)]
pub struct Editable<T> {
    /// The content
    content: T,
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

impl<T> DerefMut for Editable<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.content
    }
}

// ============================================================================
// Convenience type aliases
// ============================================================================

/// Type alias for immutable stored entities: ID + creation timestamp + content.
///
/// Internal structure: `Keyed<Id, Timestamped<T>>`
pub type Stored<Id, T> = Keyed<Id, Timestamped<T>>;

/// Type alias for mutable stored entities: ID + creation timestamp + content + updated_at.
///
/// Internal structure: `Keyed<Id, Editable<Timestamped<T>>>`
/// The Editable wrapper is outermost so updated_at tracks when the timestamped content changed.
pub type StoredEditable<Id, T> = Keyed<Id, Editable<Timestamped<T>>>;

/// Convenience constructor for Stored<Id, T>
///
/// This is the most common pattern: an entity with ID and creation timestamp.
pub fn stored<Id, T>(id: Id, content: T, created_at: i64) -> Stored<Id, T> {
    Keyed::new(id, Timestamped::new(content, created_at))
}

/// Convenience constructor for StoredEditable<Id, T>
///
/// For entities that can be modified after creation.
pub fn stored_editable<Id, T>(
    id: Id,
    content: T,
    created_at: i64,
    updated_at: i64,
) -> StoredEditable<Id, T> {
    Keyed::new(id, Editable::new(Timestamped::new(content, created_at), updated_at))
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
    fn test_keyed_deref() {
        let keyed = Keyed::new(
            TestId("test-1".to_string()),
            TestContent {
                name: "foo".to_string(),
                value: 42,
            },
        );

        // Deref allows direct access to content fields
        assert_eq!(keyed.name, "foo");
        assert_eq!(keyed.value, 42);
    }

    #[test]
    fn test_timestamped() {
        let timestamped = Timestamped::new(
            TestContent {
                name: "bar".to_string(),
                value: 100,
            },
            2000,
        );

        assert_eq!(timestamped.created_at(), 2000);
        assert_eq!(timestamped.name, "bar");
        assert_eq!(timestamped.value, 100);
    }

    #[test]
    fn test_stored_composition() {
        // Stored<Id, T> = Keyed<Id, Timestamped<T>>
        let entity = stored(
            TestId("test-2".to_string()),
            TestContent {
                name: "baz".to_string(),
                value: 200,
            },
            3000,
        );

        // Access ID
        assert_eq!(entity.id().0, "test-2");

        // Access created_at (through first Deref to Timestamped)
        assert_eq!(entity.created_at, 3000);

        // Access content fields (through double Deref: Keyed -> Timestamped -> T)
        assert_eq!(entity.name, "baz");
        assert_eq!(entity.value, 200);
    }

    #[test]
    fn test_keyed_map() {
        let keyed = Keyed::new(TestId("test-4".to_string()), 10i32);

        let mapped = keyed.map(|v| v * 2);
        assert_eq!(*mapped, 20);
        assert_eq!(mapped.id().0, "test-4");
    }

    #[test]
    fn test_editable() {
        let editable = Editable::new(
            TestContent {
                name: "edit".to_string(),
                value: 50,
            },
            5000,
        );

        assert_eq!(editable.updated_at(), 5000);
        assert_eq!(editable.name, "edit");
    }

    #[test]
    fn test_stored_editable() {
        let entity = stored_editable(
            TestId("doc-1".to_string()),
            TestContent {
                name: "document".to_string(),
                value: 1,
            },
            1000, // created_at
            2000, // updated_at
        );

        assert_eq!(entity.id().0, "doc-1");
        assert_eq!(entity.created_at, 1000);
        assert_eq!(entity.updated_at, 2000);
        assert_eq!(entity.name, "document");
    }
}
