//! Opaque Discord identifier types for MCP tool parameters.
//!
//! Each type carries its own JSON schema description via `schemars::JsonSchema`,
//! enabling automatic MCP tool definition generation via rmcp's `#[tool]` macro.

use schemars::JsonSchema;
use serde::Deserialize;

macro_rules! discord_id {
    ($name:ident, $desc:literal, $serenity_ty:path) => {
        #[derive(Debug, Clone, Copy, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            pub fn serenity(self) -> $serenity_ty {
                <$serenity_ty>::new(self.0)
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
                let mut map = serde_json::Map::new();
                map.insert("type".into(), "integer".into());
                map.insert("description".into(), $desc.into());
                map.into()
            }

            fn inline_schema() -> bool {
                true
            }
        }
    };
}

discord_id!(ChannelId, "Discord channel ID", serenity::model::id::ChannelId);
discord_id!(GuildId,   "Discord guild (server) ID", serenity::model::id::GuildId);
discord_id!(MessageId, "Discord message ID", serenity::model::id::MessageId);
discord_id!(UserId,    "Discord user ID", serenity::model::id::UserId);
