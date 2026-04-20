//! /mcp — MCP server management from Discord.

use lumina_macros::command_group;
use serenity::all::{CommandInteraction, CreateInteractionResponse, CreateInteractionResponseMessage};
use simply_daemon_api::McpApi;

use super::LuminaContext;

#[command_group(description = "MCP server management")]
mod mcp {
    use super::*;

    #[sub_command(description = "List configured MCP servers")]
    pub async fn list(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
    ) -> anyhow::Result<()> {
        let servers = lx.daemon.mcp().list_mcp_servers().await?;

        if servers.is_empty() {
            return reply_ephemeral(lx, cmd, "No MCP servers configured.").await;
        }

        let lines: Vec<String> = servers.iter().map(|s| {
            let status = if s.is_connected {
                format!("✅ connected ({} tools)", s.tool_count)
            } else if s.needs_oauth_login {
                "🔑 needs OAuth login".to_string()
            } else {
                format!("❌ {}", s.status)
            };
            format!("`{}` — {} — {}", s.id, s.name, status)
        }).collect();

        reply_ephemeral(lx, cmd, &lines.join("\n")).await
    }

    #[sub_command(description = "Add a new MCP server")]
    pub async fn add(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("Unique server ID (e.g. 'my-server')")] id: String,
        #[describe("Server name")] name: String,
        #[describe("Server URL")] url: String,
        #[describe("Auth type: none, oauth, or bearer")] auth_type: Option<String>,
    ) -> anyhow::Result<()> {
        let auth = auth_type.unwrap_or_else(|| "none".to_string());

        lx.daemon.mcp().add_mcp_server(simply_daemon_api::AddMcpServerRequest {
            id: id.clone(),
            name: name.clone(),
            url,
            auth_type: auth,
            auth_token: None,
            provider_id: None,
            scopes: None,
        }).await?;

        reply_ephemeral(lx, cmd, &format!("Added MCP server `{id}` ({name}).")).await
    }

    #[sub_command(description = "Remove an MCP server")]
    pub async fn remove(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("Server ID to remove")] id: String,
    ) -> anyhow::Result<()> {
        lx.daemon.mcp().remove_mcp_server(&id).await?;
        reply_ephemeral(lx, cmd, &format!("Removed MCP server `{id}`.")).await
    }

    #[sub_command(description = "Connect to an MCP server")]
    pub async fn connect(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("Server ID to connect")] id: String,
    ) -> anyhow::Result<()> {
        let tool_count = lx.daemon.mcp().connect_mcp_server(&id).await?;
        reply_ephemeral(lx, cmd, &format!("Connected to `{id}` — {tool_count} tools available.")).await
    }

    #[sub_command(description = "Disconnect from an MCP server")]
    pub async fn disconnect(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("Server ID to disconnect")] id: String,
    ) -> anyhow::Result<()> {
        lx.daemon.mcp().disconnect_mcp_server(&id).await?;
        reply_ephemeral(lx, cmd, &format!("Disconnected from `{id}`.")).await
    }

    #[sub_command(description = "List tools from a connected server")]
    pub async fn tools(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        #[describe("Server ID")] id: String,
    ) -> anyhow::Result<()> {
        let tools = lx.daemon.mcp().get_mcp_server_tools(&id).await?;

        if tools.is_empty() {
            return reply_ephemeral(lx, cmd, &format!("No tools available on `{id}`.")).await;
        }

        let lines: Vec<String> = tools.iter().map(|t| {
            let desc = t.description.as_ref().map(|d| d.to_string()).unwrap_or_default();
            format!("`{}` — {}", t.name, desc)
        }).collect();

        let text = format!("**Tools on `{id}`** ({}):\n{}", tools.len(), lines.join("\n"));
        let pages = crate::paginator::paginate_text(&text, 1900);
        crate::paginator::send_paginated(lx, cmd, &pages, std::time::Duration::from_secs(120)).await
    }

    async fn reply_ephemeral(
        lx: &LuminaContext,
        cmd: &CommandInteraction,
        content: &str,
    ) -> anyhow::Result<()> {
        cmd.create_response(
            &lx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .ephemeral(true),
            ),
        )
        .await?;
        Ok(())
    }
}
