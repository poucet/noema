//! MCP parameter types — trait + opaque Discord identifiers.
//!
//! Each type carries its own JSON schema and description for automatic
//! MCP tool definition generation via `#[mcp_tools]`.

use anyhow::{Context as _, Result};
use serde_json::{json, Value};

/// A type that can appear as an MCP tool parameter.
///
/// Provides the JSON schema type, a default description (used when no
/// `#[describe]` annotation overrides it), and deserialization from JSON.
pub trait McpParam: Sized {
    /// JSON schema fragment for this type (e.g. `{"type": "integer"}`).
    fn schema() -> Value;

    /// Default description used in the tool schema.
    /// Return `""` to require an explicit `#[describe("...")]`.
    fn default_description() -> &'static str;

    /// Parse from a JSON value.
    fn from_value(value: &Value) -> Result<Self>;
}

// ---------------------------------------------------------------------------
// Primitive impls
// ---------------------------------------------------------------------------

impl McpParam for String {
    fn schema() -> Value { json!({"type": "string"}) }
    fn default_description() -> &'static str { "" }
    fn from_value(v: &Value) -> Result<Self> {
        v.as_str().map(String::from).context("expected string")
    }
}

impl McpParam for u64 {
    fn schema() -> Value { json!({"type": "integer"}) }
    fn default_description() -> &'static str { "" }
    fn from_value(v: &Value) -> Result<Self> {
        v.as_u64().context("expected integer")
    }
}

impl McpParam for bool {
    fn schema() -> Value { json!({"type": "boolean"}) }
    fn default_description() -> &'static str { "" }
    fn from_value(v: &Value) -> Result<Self> {
        v.as_bool().context("expected boolean")
    }
}

impl<T: McpParam> McpParam for Vec<T> {
    fn schema() -> Value { json!({"type": "array", "items": T::schema()}) }
    fn default_description() -> &'static str { "" }
    fn from_value(v: &Value) -> Result<Self> {
        v.as_array()
            .context("expected array")?
            .iter()
            .map(T::from_value)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Discord opaque types — strongly typed IDs with automatic schema descriptions
// ---------------------------------------------------------------------------

macro_rules! discord_id {
    ($name:ident, $desc:literal, $serenity_ty:path) => {
        pub struct $name(pub u64);

        impl McpParam for $name {
            fn schema() -> Value { json!({"type": "integer"}) }
            fn default_description() -> &'static str { $desc }
            fn from_value(v: &Value) -> Result<Self> {
                v.as_u64().map(Self).context(concat!("expected ", $desc))
            }
        }

        impl $name {
            pub fn serenity(self) -> $serenity_ty {
                <$serenity_ty>::new(self.0)
            }
        }
    };
}

discord_id!(ChannelId, "Discord channel ID", serenity::model::id::ChannelId);
discord_id!(GuildId,   "Discord guild (server) ID", serenity::model::id::GuildId);
discord_id!(MessageId, "Discord message ID", serenity::model::id::MessageId);
discord_id!(UserId,    "Discord user ID", serenity::model::id::UserId);
