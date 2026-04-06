//! Opaque Discord identifier types for MCP tool parameters.
//!
//! Each type carries its own JSON schema description via `schemars::JsonSchema`,
//! enabling automatic MCP tool definition generation via rmcp's `#[tool]` macro.

use schemars::JsonSchema;
use serde::Deserialize;

macro_rules! discord_id {
    ($name:ident, $desc:literal, $serenity_ty:path) => {
        #[derive(Debug, Clone, Copy)]
        pub struct $name(pub u64);

        impl $name {
            pub fn serenity(self) -> $serenity_ty {
                <$serenity_ty>::new(self.0)
            }
        }

        /// Deserialize from string or integer — LLMs may send either.
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                use serde::de;
                struct Visitor;
                impl de::Visitor<'_> for Visitor {
                    type Value = u64;
                    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                        write!(f, "a string or integer Discord ID")
                    }
                    fn visit_u64<E: de::Error>(self, v: u64) -> Result<u64, E> { Ok(v) }
                    fn visit_i64<E: de::Error>(self, v: i64) -> Result<u64, E> { Ok(v as u64) }
                    fn visit_f64<E: de::Error>(self, v: f64) -> Result<u64, E> { Ok(v as u64) }
                    fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
                        v.parse().map_err(de::Error::custom)
                    }
                }
                Ok($name(deserializer.deserialize_any(Visitor)?))
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
                let mut map = serde_json::Map::new();
                map.insert("type".into(), "string".into());
                map.insert("description".into(), concat!($desc, " (as string)").into());
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
