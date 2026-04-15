# Per-User MCP OAuth + Google Docs Import

**Status:** planned
**Version:** 1.0
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)
**Related:** [AUTH_AND_IDENTITY.md](AUTH_AND_IDENTITY.md), [EMBEDDING_AND_RAG.md](EMBEDDING_AND_RAG.md)

---

## Problem

MCP servers that access user data (Google Docs, Notion, GitHub) need the *user's* OAuth token, not a daemon-level credential. Today there's no mechanism for per-user, per-MCP-server OAuth. Without it, every new service requires custom daemon code instead of being a standard MCP server.

Google Docs import is the first concrete use case, but the solution must be generic — user A might have Google tokens for a docs server AND Notion tokens for a Notion server.

## Goals

- Generic per-user, per-MCP-server transient OAuth token management
- Token injection: daemon adds user's token when calling MCP servers
- `auth_required` error flow when no token exists
- Google Docs import as the first consumer
- Tokens in-memory only — no persistence, GDPR-safe
- OAuth flows run on the daemon HTTP port (configurable for cloud hosting)
- Admin UI config for OAuth credentials and public URL
- Lumina commands for auth + import with autocomplete

## Non-Goals

- Persistent token storage
- Automatic token refresh (re-auth on expiry)
- Agent-initiated imports (user action only, for now)
- Google Docs write access

---

## Architecture

```
  Discord / Noema / Admin UI
        │
        ▼
  ┌─────────────────────────────────────────────┐
  │              simply-daemon                    │
  │                                               │
  │  TransientTokenStore                          │
  │    (user_id, server_id) → access_token        │
  │                                               │
  │  /auth/mcp/{server_id}?user_id=...            │
  │    → OAuth provider (Google, Notion, etc.)     │
  │    → callback → store token in-memory          │
  │                                               │
  │  MCP tool call with user context:              │
  │    1. Look up token for (user_id, server_id)   │
  │    2. Has token → inject Authorization header  │
  │    3. No token → return auth_required + URL    │
  └──────────┬────────────────────────────────────┘
             │ Authorization: Bearer {user_token}
             ▼
  ┌──────────────────────┐  ┌──────────────────────┐
  │  noema-mcp-gdocs     │  │  notion-mcp (future) │
  │  Google Docs tools   │  │  Notion tools        │
  └──────────────────────┘  └──────────────────────┘
```

---

## Components

### 1. MCP Server OAuth Config

MCP servers declare their OAuth requirements in their config (already partially exists):

```toml
[mcp_servers.google-docs]
url = "http://localhost:9877/mcp"
auth = "oauth"
client_id = "xxxx.apps.googleusercontent.com"
client_secret = "GOCSPX-..."
authorization_url = "https://accounts.google.com/o/oauth2/v2/auth"
token_url = "https://oauth2.googleapis.com/token"
scopes = ["https://www.googleapis.com/auth/drive.readonly", "https://www.googleapis.com/auth/documents.readonly"]
```

The `client_id` and `client_secret` are configured per MCP server via the admin UI, same pattern as API keys. The daemon reads these when initiating OAuth flows.

### 2. Transient Token Store

In-memory map on the daemon, generic across all MCP servers:

```rust
struct TransientTokenStore {
    // (user_id, server_id) → token
    tokens: Mutex<HashMap<(UserId, String), McpUserToken>>,
}

struct McpUserToken {
    access_token: String,
    expires_at: Option<Instant>,
    // email or display name from the OAuth provider (for status display)
    identity: Option<String>,
}
```

- Tokens never written to disk
- Daemon restart = all tokens gone, users re-auth
- Tokens expire naturally
- No refresh tokens — user re-auths when expired

### 3. OAuth Flow

**Endpoint:** `GET /auth/mcp/{server_id}?user_id={user_id}`

1. Daemon looks up the MCP server's OAuth config
2. Builds OAuth authorization URL with:
   - `client_id` and `scopes` from server config
   - `redirect_uri` = `{public_url}/auth/mcp/callback`
   - `state` = encoded `{user_id, server_id}`
3. Redirects user's browser to the OAuth provider
4. User consents
5. Provider redirects to `/auth/mcp/callback?code=...&state=...`
6. Daemon exchanges code for access token using `token_url` from config
7. Stores token in `TransientTokenStore` keyed by `(user_id, server_id)`
8. Shows "Connected!" page

### 4. Token Injection

When the daemon calls an MCP server on behalf of a user:

1. Check `TransientTokenStore` for `(user_id, server_id)`
2. **Has valid token** → add `Authorization: Bearer {access_token}` to the MCP request
3. **No token or expired** → return structured error to the client:
   ```json
   {"error": "auth_required", "server_id": "google-docs", "auth_url": "https://daemon:9800/auth/mcp/google-docs?user_id=xxx"}
   ```

The MCP server receives the token as a standard HTTP Authorization header and uses it for its API calls.

### 5. Admin Configuration

Settings in `settings.toml`:

```toml
# Cloud hosting — used for OAuth callback URLs
# Defaults to http://localhost:{daemon_port}
public_url = "https://my-server.example.com"
```

Per-MCP-server OAuth credentials configured via admin UI:
- Admin page shows each MCP server's OAuth config (client_id, client_secret)
- Same input pattern as API keys — password field + save button
- Stored in the MCP server config in `settings.toml`

### 6. Google Docs Import (first consumer)

**Import endpoint on daemon:**

```rust
#[rpc_service("google")]
pub trait GoogleApi: Send + Sync {
    /// Check if the current user has a valid Google token.
    #[rpc(get = "/google/status")]
    async fn google_status(&self) -> Result<GoogleAuthStatus>;

    /// List the user's Google Docs (requires Google token).
    #[rpc(get = "/google/docs")]
    async fn list_google_docs(&self) -> Result<Vec<GoogleDocInfo>>;

    /// Import a Google Doc into UCM storage.
    #[rpc(post = "/google/import")]
    async fn import_google_doc(&self, request: ImportGoogleDocRequest) -> Result<DocumentInfo>;
}
```

The `GoogleService` implementation:
1. Gets user's Google token from `TransientTokenStore`
2. Uses `GoogleDocsClient` (from `noema-mcp-gdocs` crate, used as library) with the token
3. Extracts doc content + images
4. Stores as UCM document with `type: knowledge`, `source: google_drive`
5. Re-import: matches by `source_id`, updates existing document (delete old tabs, create new)
6. Embedding queue picks up new tabs automatically

**Note:** The `GoogleApi` is a daemon REST endpoint, not an MCP tool. It calls the Google API directly using the user's token from the transient store. This is separate from the `noema-mcp-gdocs` MCP server — which is for generic MCP clients that want Google Docs tools.

### 7. Lumina Discord Commands

**`/google auth`**
- Generates URL: `{public_url}/auth/mcp/google-docs?user_id={ucm_user_id}`
- Posts ephemeral message with clickable link
- If user has no UCM user_id → prompts `/auth` first

**`/google import`**
- If user not authed with Google → tells them to `/google auth` first
- Options:
  - `doc_id` with autocomplete — calls `list_google_docs()`, shows user's docs
  - `url` — paste a Google Docs URL, extracts doc_id
- Calls `import_google_doc(doc_id)`
- Shows result embed: title, tab count, "now searchable via RAG"

**`/google status`**
- Shows whether Google is connected, email, time until re-auth

### 8. Admin UI

- Settings page: `public_url` field
- MCP server config: per-server OAuth credentials (client_id, client_secret)
- Documents page: "Import Google Doc" button
- User status: which MCP servers each user is connected to

---

## Decisions

1. **Transient tokens only** — no database storage, GDPR-safe. Re-auth on daemon restart or token expiry.
2. **Generic per-MCP-server OAuth** — not Google-specific. Same mechanism works for Notion, GitHub, etc.
3. **Token injection via Authorization header** — MCP servers receive user tokens as standard HTTP auth.
4. **Google import is a daemon endpoint, not MCP tool** — user action, not agent action. Can be promoted later.
5. **Re-import = update** — matched by `source_id`, old tabs deleted, new tabs created, re-embedded.
6. **`public_url` in settings** — defaults to `http://localhost:{daemon_port}`, configurable for cloud hosting.
