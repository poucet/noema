//! Discord OAuth2 invite URL for adding Lumina to a server.
//!
//! The permission set is computed from what Lumina *actually* does —
//! kept in sync with `chat.rs`, `mcp/tools.rs`, and the voice code
//! paths. Asking for only what we use keeps the "Add to server"
//! dialog honest and avoids scary-looking permission grants that
//! don't correspond to any behaviour.

use serenity::all::Permissions;

/// Union of every Discord permission Lumina needs at runtime.
///
/// Derived from:
///
/// | Permission            | Why                                                                |
/// |-----------------------|--------------------------------------------------------------------|
/// | `VIEW_CHANNEL`        | read messages, list channels, see voice states, seed history       |
/// | `READ_MESSAGE_HISTORY`| chat history → RAG seed, `search_messages`, `get_channel_history`  |
/// | `SEND_MESSAGES`       | assistant replies, tool-call/result embeds, `send_message` tool    |
/// | `EMBED_LINKS`         | tool-call / tool-result / error embeds                             |
/// | `ATTACH_FILES`        | image / audio tool results, voice transcription output             |
/// | `ADD_REACTIONS`       | `create_poll` tool                                                 |
/// | `USE_EXTERNAL_EMOJIS` | `get_emoji_list` + polls that reference custom emojis              |
/// | `CONNECT`             | voice `join_voice`                                                 |
/// | `SPEAK`               | voice output (Songbird)                                            |
/// | `USE_VAD`             | voice activity detection on incoming audio                         |
///
/// Slash-command registration is granted by the `applications.commands`
/// OAuth scope, not a permission bit — no entry here for that.
pub fn required_permissions() -> Permissions {
    Permissions::VIEW_CHANNEL
        | Permissions::READ_MESSAGE_HISTORY
        | Permissions::SEND_MESSAGES
        | Permissions::EMBED_LINKS
        | Permissions::ATTACH_FILES
        | Permissions::ADD_REACTIONS
        | Permissions::USE_EXTERNAL_EMOJIS
        | Permissions::CONNECT
        | Permissions::SPEAK
        | Permissions::USE_VAD
}

/// Build the Discord OAuth2 invite URL for Lumina. Paste in a browser
/// while logged in as someone with **Manage Server** on the target
/// guild; Discord walks you through the "Add to server" dialog.
pub fn invite_url(app_id: u64) -> String {
    let perms = required_permissions().bits();
    format!(
        "https://discord.com/oauth2/authorize\
         ?client_id={app_id}\
         &scope=bot+applications.commands\
         &permissions={perms}"
    )
}
