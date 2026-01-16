//! Type-safe ID newtypes for storage entities
//!
//! All IDs are UUIDs wrapped in newtypes for compile-time safety.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Macro to define a type-safe ID newtype
macro_rules! define_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Create a new random ID
            pub fn new() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            /// Create from an existing string (for loading from DB)
            pub fn from_string(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Get the inner string value
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consume and return the inner string
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }

        #[cfg(feature = "rusqlite")]
        impl rusqlite::types::FromSql for $name {
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                value.as_str().map(|s| Self(s.to_string()))
            }
        }

        #[cfg(feature = "rusqlite")]
        impl rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(rusqlite::types::ToSqlOutput::Borrowed(
                    rusqlite::types::ValueRef::Text(self.0.as_bytes()),
                ))
            }
        }
    };
}

// Entities (addressable layer) - must be defined first since ConversationId is an alias
define_id!(EntityId, "Unique identifier for an addressable entity");

// Content & Assets
define_id!(ContentBlockId, "Unique identifier for a content block");
define_id!(AssetId, "Unique identifier for a binary asset (SHA-256 hash)");

// Conversations (ConversationId is now an alias for EntityId)
/// Type alias for backward compatibility - conversations are now entities
pub type ConversationId = EntityId;
define_id!(TurnId, "Unique identifier for a turn in a conversation");
define_id!(SpanId, "Unique identifier for a span (alternative response)");
define_id!(MessageId, "Unique identifier for a message within a span");
define_id!(MessageContentId, "Unique identifier for a content item within a message");
define_id!(ViewId, "Unique identifier for a view (path through alternatives)");

// Documents
define_id!(DocumentId, "Unique identifier for a document");
define_id!(TabId, "Unique identifier for a document tab");
define_id!(RevisionId, "Unique identifier for a tab revision");

// Collections
define_id!(CollectionId, "Unique identifier for a collection");
define_id!(CollectionItemId, "Unique identifier for a collection item");

// References
define_id!(ReferenceId, "Unique identifier for a cross-reference");

// Users
define_id!(UserId, "Unique identifier for a user");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_creation() {
        let id1 = ContentBlockId::new();
        let id2 = ContentBlockId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_id_from_string() {
        let id = ContentBlockId::from_string("test-id-123");
        assert_eq!(id.as_str(), "test-id-123");
    }

    #[test]
    fn test_id_display() {
        let id = UserId::from_string("user-abc");
        assert_eq!(format!("{}", id), "user-abc");
    }

    #[test]
    fn test_id_serde() {
        let id = DocumentId::from_string("doc-123");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"doc-123\"");

        let parsed: DocumentId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, id);
    }
}
