# Google Docs Import

**Status:** refined
**Version:** 1.0
**Parent:** [ARCHITECTURE.md](ARCHITECTURE.md)
**Related:** [AUTH_AND_IDENTITY.md](AUTH_AND_IDENTITY.md), [EMBEDDING_AND_RAG.md](EMBEDDING_AND_RAG.md)

---

## Problem

Google Docs import only works from Noema via hand-wired Tauri commands that bypass the daemon. Lumina users can't import docs at all. The import logic uses hardcoded OAuth that doesn't work for multiple users or cloud hosting.

We need Google Docs import as a daemon-level capability that any client (Lumina, Noema, admin UI) can use, with per-user transient OAuth that doesn't persist tokens to disk (GDPR-safe).

## Goals

- Import Google Docs from Discord via `/google import` with autocomplete
- Import from admin UI
- Transient OAuth — tokens in-memory only, daemon restart = re-auth
- Google OAuth credentials configured via admin UI (like API keys)
- Cloud-hosting friendly — configurable public URL for OAuth callbacks
- Imported docs stored as UCM documents (`type: knowledge`) with auto-embedding

## Non-Goals

- Persistent token storage (GDPR concern)
- Agent-initiated imports (user action only, for now)
- Google Docs write access
- Syncing/updating previously imported docs (re-import = new version)

---

## Architecture

```
  Discord                     Admin UI
  /google auth ──┐            Settings page
  /google import ─┤            ├─ client_id/secret config
                  │            ├─ /google/import button
                  ▼            ▼
            ┌─────────────────────────┐
            │    simply-daemon        │
            │                         │
            │  /auth/google           │ ← OAuth callback (on daemon port)
            │  /google/docs           │ ← List user's docs
            │  /google/import         │ ← Extract + store + embed
            │                         │
            │  In-memory token map:   │
            │  user_id → GoogleToken  │
            └─────────────────────────┘
```

---

## Components

### 1. Admin Configuration

Google OAuth credentials added to `settings.toml`, configured via admin UI using the same pattern as API keys:

```toml
# Google OAuth (for Google Docs import)
google_client_id = "xxxx.apps.googleusercontent.com"
google_client_secret = "GOCSPX-..."

# Cloud hosting — used for OAuth callback URLs
# Defaults to http://localhost:{daemon_port}
public_url = "https://my-server.example.com"
```

Admin UI shows these in the Settings card alongside API keys. No code changes to the key management pattern — just new fields.

### 2. Transient OAuth Token Store

In-memory map on the daemon:

```rust
struct GoogleTokenStore {
    tokens: HashMap<UserId, GoogleToken>,
}

struct GoogleToken {
    access_token: String,
    expires_at: Instant,
}
```

- Tokens are never written to disk or database
- Daemon restart = all tokens gone, users re-auth
- Tokens expire naturally (Google access tokens are ~1 hour)
- No refresh tokens stored — user re-auths when token expires

### 3. OAuth Flow

**Endpoint:** `GET /auth/google?user_id={user_id}`

1. Daemon builds Google OAuth URL with:
   - `client_id` from settings
   - `redirect_uri` = `{public_url}/auth/google/callback`
   - `scope` = `https://www.googleapis.com/auth/drive.readonly https://www.googleapis.com/auth/documents.readonly`
   - `state` = encoded `{user_id}`
2. Redirects user's browser to Google
3. User consents
4. Google redirects to `/auth/google/callback?code=...&state=...`
5. Daemon exchanges code for access token
6. Stores token in-memory keyed by user_id
7. Shows "Connected!" page (or redirects back to Discord/admin)

### 4. Google Docs API (daemon REST)

New `GoogleApi` trait on the daemon:

```rust
#[rpc_service("google")]
pub trait GoogleApi: Send + Sync {
    /// Check if the current user has a valid Google token.
    #[rpc(get = "/google/status")]
    async fn google_status(&self) -> Result<GoogleAuthStatus>;

    /// List the user's Google Docs.
    #[rpc(get = "/google/docs")]
    async fn list_google_docs(&self) -> Result<Vec<GoogleDocInfo>>;

    /// Import a Google Doc into UCM storage.
    /// Extracts content + images, stores as document with type "knowledge",
    /// triggers embedding automatically.
    #[rpc(post = "/google/import")]
    async fn import_google_doc(&self, request: ImportGoogleDocRequest) -> Result<DocumentInfo>;
}

struct GoogleAuthStatus {
    authenticated: bool,
    email: Option<String>,   // from token info
    expires_in: Option<u64>, // seconds until re-auth needed
}

struct GoogleDocInfo {
    id: String,
    title: String,
    modified_time: String,
}

struct ImportGoogleDocRequest {
    doc_id: String,
}
```

The import endpoint:
1. Checks user has a valid Google token (returns `auth_required` error if not)
2. Uses `GoogleDocsClient` (from `noema-mcp-gdocs/src/google_api.rs`) to extract the doc
3. Creates UCM document with `type: knowledge`, `source: google_drive`, `source_id: {doc_id}`
4. Stores images as assets
5. Creates tabs with markdown content
6. Embedding queue picks up the new tabs automatically
7. Returns the created `DocumentInfo`

### 5. Lumina Discord Commands

**`/google auth`**
- Generates URL: `{public_url}/auth/google?user_id={ucm_user_id}`
- Posts ephemeral message with clickable link
- If user has no UCM user_id yet, prompts them to `/auth` first

**`/google import`**
- If user not authed with Google → tells them to `/google auth` first
- Subcommand options:
  - `doc_id` with autocomplete — calls `list_google_docs()`, shows user's docs as choices
  - `url` — paste a Google Docs URL, extracts the doc_id from it
- Calls `import_google_doc(doc_id)`
- Shows result embed with document title, tab count, "now searchable via RAG"

**`/google status`**
- Shows whether Google is connected, email, time until re-auth

### 6. Admin UI

- Settings page: fields for `google_client_id`, `google_client_secret`, `public_url`
- Documents page: "Import Google Doc" button that triggers the same flow
- Google auth status indicator in the header/sidebar

---

## Decisions

1. **Transient tokens only** — no database storage, GDPR-safe. Re-auth on daemon restart or token expiry.
2. **Import is a user action** — REST endpoint, not MCP tool. Can be promoted to MCP tool later.
3. **Reuse `GoogleDocsClient`** — the extraction logic in `noema-mcp-gdocs/src/google_api.rs` is used directly from the daemon, no MCP hop.
4. **Public URL config** — defaults to `http://localhost:{daemon_port}`, configurable for cloud hosting via admin UI.

## Open Questions

1. **Re-import behavior** — if the same Google Doc is imported again, update the existing document (matched by `source_id`) or create a new one? Leaning toward update (delete old tabs, create new ones).
