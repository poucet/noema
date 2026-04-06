# Auth & Multi-User Identity

**Status:** draft
**Priority:** P0
**Depends on:** Foundation (complete)

---

## Problem

The daemon is becoming a shared service — cloud-deployable, accessed by multiple clients (Noema, Lumina, future), serving multiple users (Discord users, desktop user). Today it has zero auth: everything is trusted localhost with a single global user.

This breaks when:
- The daemon runs in the cloud — anyone who knows the port can connect
- Lumina serves multiple Discord users — all operations use one user_id
- Users need Google access — OAuth tokens are per-user, not per-daemon
- Documents need ownership — who created what, who can see what

## Goals

1. **Minimal setup** — adding a new Discord user should require zero admin work
2. **Connection security** — only authorized clients can talk to the daemon
3. **Per-user data** — users who authenticate (via OAuth) get their own document storage
4. **Public content** — some documents are available to all users (no auth required to read)
5. **Admin = Google account** — a single Google email in the config is the admin
6. **Single port** — OAuth flows, admin page, REST API, WebSocket all on one port
7. **OAuth via web** — users click a link, authenticate on the daemon's admin page, done

## Non-Goals

- Multi-tenant isolation (separate databases per user)
- Fine-grained ACL/permissions (read/write/admin per document)
- Federated identity (multiple OAuth providers for user login — Google only for now)
- UCM accounts for every Discord user (only authenticated users get storage)

## Design

### User Tiers

| Tier | Who | Gets | How |
|------|-----|------|-----|
| **Admin** | Google account in `settings.toml` | Full access, admin page, all documents | Config: `admin_email = "you@gmail.com"` |
| **Authenticated user** | Anyone who completes OAuth on the admin page | Own documents, own Google tokens, own conversations | Click link → Google login → daemon creates UCM user |
| **Anonymous** | Discord users who haven't authenticated | Public documents (read-only), conversations (ephemeral), voice | No auth needed |

### Connection Auth

**Clients (Noema, Lumina)** connect with a **shared secret** — a random token generated on first daemon startup, stored in `settings.toml`:

```toml
daemon_secret = "randomly-generated-on-first-run"
```

Clients send it as `Authorization: Bearer {daemon_secret}` on every request. This proves the client is authorized to talk to the daemon. It does NOT identify a user — it identifies a trusted client.

- **Noema**: reads the secret from the same `settings.toml` (shared filesystem)
- **Lumina**: configured in `lumina.toml` with the daemon secret
- **Cloud**: the secret is set in the deployment config

This is the simplest model that works. No certificates, no JWT rotation, no key management.

### User Identity

Every HTTP request can optionally carry a user identity:

```
X-User-Id: {ucm_user_id}
```

- **Noema**: sends the admin's user_id (from config) on every request
- **Lumina**: sends the Discord user's mapped UCM user_id if they've authenticated, otherwise omitted (anonymous)
- **Admin page**: sets user from the OAuth session cookie
- **No header** = anonymous user (public access only)

The daemon resolves the user_id to a UCM user record. If the user_id is invalid, the request is treated as anonymous.

### OAuth Flow (User Authentication)

When a Discord user wants to authenticate (to get their own documents, connect Google):

1. User types `/auth` in Discord (or clicks a link Lumina provides)
2. Lumina generates a URL: `https://daemon:9800/auth/login?discord_id=12345&redirect=discord`
3. User clicks the link → daemon's admin page
4. Page shows "Sign in with Google" button
5. User completes Google OAuth
6. Daemon creates/updates UCM user (email from Google profile)
7. Daemon maps Discord ID → UCM user_id
8. Daemon redirects back to Discord (or shows "You're connected!")
9. Future requests from this Discord user include their UCM user_id

### Admin Page Auth

The admin page at `https://daemon:9800/admin` is protected:

1. Accessing `/admin` redirects to Google OAuth
2. After login, daemon checks if the Google email matches `admin_email` in settings
3. If yes → admin session (cookie). If no → regular user session.
4. Admin can: view connections, manage MCP servers, see all users
5. Regular user can: manage their own Google tokens, see their documents

### MCP Tool Call User Context

When the agent calls an MCP tool (e.g., Google Docs):

1. The session carries a `user_id`
2. The daemon looks up the user's OAuth tokens
3. The token is passed to the MCP server in the request headers
4. If the user has no token for that service → tool returns "Please authenticate: {link}"

### Document Ownership

- Documents created by an authenticated user belong to that user
- Documents can be marked `is_public: true` (visible to all, editable by owner)
- Anonymous users can read public documents but not create/edit
- Admin can see/edit all documents

### Discord User Mapping

Stored in the database (new table or entity metadata):

```
discord_user_id → ucm_user_id (nullable)
```

- First interaction: Discord user is anonymous (no mapping)
- After `/auth`: mapping created, persisted
- Lumina resolves Discord ID → UCM user_id on each request
- If no mapping → anonymous (public access only)

## Request Flow

```
Discord user speaks in voice channel
  → Lumina resolves discord_id → ucm_user_id (or anonymous)
  → REST/WS request to daemon with:
      Authorization: Bearer {daemon_secret}     ← client auth
      X-User-Id: {ucm_user_id}                  ← user identity (optional)
  → Daemon validates secret, resolves user
  → All operations scoped to that user
  → Tool calls include user's OAuth tokens
```

## Migration

- **Phase 1**: Add `daemon_secret` (auto-generated). Clients send it. Daemon validates. No user identity yet — existing single-user behavior unchanged.
- **Phase 2**: Add `X-User-Id` header support. Noema sends admin user. Lumina sends mapped users. Document operations scoped.
- **Phase 3**: Admin page OAuth. User self-service auth. Discord user mapping.
- **Phase 4**: Per-user MCP tokens. Google Docs import per-user.

### MCP Service Access Control via Discord Roles

MCP services (Google Docs, GitHub, etc.) can be restricted to specific Discord roles. Configured in `lumina.toml`:

```toml
[mcp_access]
# Role name → list of MCP server IDs they can use
admin = ["*"]                           # admin role gets everything
developers = ["github", "google-docs"]  # devs get GitHub + Docs
everyone = ["google-docs"]              # everyone gets Docs
```

**How it works:**
1. Lumina checks the Discord user's roles on each request
2. Before calling an MCP tool, Lumina checks if the user's role has access to that MCP server
3. If not → "You don't have access to this tool. Ask an admin to grant the `{role}` role."
4. This is enforced at the Lumina level (client-side), not the daemon level — the daemon trusts its clients

**Why Discord roles, not daemon-level permissions:**
- Zero extra infrastructure — roles already exist in Discord
- Visual — admins can see who has what in Discord settings
- Dynamic — add/remove a role, permissions update instantly
- Per-guild — different Discord servers can have different permissions

For Noema (desktop), all MCP services are available (single admin user). The role check is Lumina-specific.

## Open Questions

1. **HTTPS** — should the daemon serve HTTPS directly (self-signed cert) or rely on a reverse proxy? For OAuth callbacks, HTTPS is required by Google.
2. **Token refresh** — how to handle expired Google tokens? Silent refresh with stored refresh token?
3. **Rate limiting** — should anonymous users be rate-limited?
4. **Revocation** — can the admin revoke a user's access?
