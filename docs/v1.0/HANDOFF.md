# v1.0 Handoff — Multi-user & Admin UI

**Date:** 2026-04-06
**Phase:** multi-user (stage 2)
**Status:** Backend complete, admin UI scaffolded, untested end-to-end

---

## What was done

### Auth & Identity (Stage 1 — complete)

- **daemon_secret**: auto-generated 32-byte token in `settings.toml` on first run
- **Auth middleware**: all routes except `/`, `/admin`, `/admin/api/*`, `/auth/*`, `/_assets/*` require `Authorization: Bearer {token}` or `simply_token` cookie
- **Two token types**: `daemon_secret` (trusted service clients — Noema, Lumina) and per-user tokens (issued after OAuth)
- **X-User-Id**: only honored from `daemon_secret` bearers (service-to-service trust). Per-user tokens resolve identity from the `user_tokens` table — non-spoofable
- **RequestUser enum**: `Service(Option<UserId>)`, `User(UserId)`, `Anonymous` — resolved by middleware, stored in axum request extensions
- **`__user_id` injection**: middleware injects resolved user_id into RPC dispatch params so handlers can access it
- **UserTier enum**: `Admin`, `Authenticated`, `Anonymous` — `admin_email` in settings determines admin
- **Document ownership**: `verify_document_access(id, require_owner)` checks on all document/tab operations. Read access allowed for owner or `is_public` docs. Mutations require owner.
- **New tables**: `user_tokens`, `discord_user_mappings`, `documents.is_public` column
- **UserStore expanded**: `get_user_by_id`, `create_user_token`, `resolve_token`, `revoke_user_tokens`, `map_discord_user`, `resolve_discord_user`
- **WS auth**: Authorization header sent on WebSocket upgrade request

### OAuth & Admin Page (Stage 2 — complete)

- **`/auth/login`**: redirects to Google OAuth. Creds resolved from: env vars (`GOOGLE_CLIENT_ID`) > `settings.toml` > built-in (future)
- **`/auth/callback`**: exchanges code, fetches Google profile, creates UCM user, maps Discord user if `discord_id` in state, issues `simply_token` cookie, redirects to `/`
- **First sign-in becomes admin**: if `admin_email` is not set, the first Google sign-in sets it + `user_email`
- **Cookie-based sessions**: `simply_token` cookie set by OAuth callback, accepted on all routes as Bearer alternative
- **Lumina `/auth` command**: ephemeral message with auth link including `discord_id`

### Admin UI

- **Astro project** at `admin/` — builds to `admin/dist/`, served by daemon
- **Setup wizard**: Step 1 (Google sign-in) → Step 2 (API keys) → Step 3 (done)
- **Dashboard**: connections, sessions, users, API keys, settings, kill button
- **Google Cloud setup guide**: step-by-step with direct links to Cloud Console pages (`/auth/branding`, `/auth/audience`, credential creation)
- **Admin API endpoints**:
  - `GET /admin/api/setup-status` — is setup complete?
  - `GET/PUT /admin/api/settings` — read/update settings
  - `POST /admin/api/api-key` — set API key
  - `DELETE /admin/api/api-key/{provider}` — remove API key
  - `GET/POST /admin/api/users` — list/create users
  - `POST /admin/api/tokens` — issue user token
  - `POST /admin/api/tokens/revoke` — revoke tokens
  - `GET /admin/api/connections` — active WS connections
  - `GET /auth/status` — Google OAuth availability

### Client wiring

- **DaemonRpcConnection**: accepts `daemon_secret` + optional `user_id`, sends both as headers on REST and WS
- **RemoteDaemon**: `connect_as(addr, name, secret, user_id)`
- **connect_or_host**: loads secret from settings, passes through
- **Noema**: builds authenticated `reqwest::Client` for asset proxy
- **bin/daemon**: runs `npm install` + `npm run build` in admin/ before starting, prints admin URL

---

## What needs testing

None of this has been tested end-to-end yet. Priority test areas:

1. **Start daemon, open admin page** — does the setup wizard render?
2. **Google OAuth flow** — configure creds, sign in, does it create user + set admin?
3. **API key management** — add/remove keys through the admin UI
4. **Auth enforcement** — unauthenticated requests rejected? Cookie auth works?
5. **Noema connects** — does it still work with the auth changes?
6. **Document ownership** — create doc as one user, verify another can't access it
7. **Lumina /auth** — does the slash command work? Does Discord mapping persist?

---

## What's next

### Immediate (before moving on)
- End-to-end test of the above
- Fix any compile errors or runtime bugs found during testing
- Built-in Google OAuth client ID for zero-config localhost (needs Google Cloud project)

### Stage 3 — Per-User MCP OAuth (not started)
- Per-user per-server token storage
- Token injection into MCP requests
- `auth_required` error when user has no token

### Stage 4 — Discord RBAC (not started)
- `[mcp_access]` config in `lumina.toml`
- Role-based tool access

### Stage 5 — Admin UI polish (not started)
- Google OAuth protection for admin page (currently open on localhost)
- User management UI (revoke access, view linked accounts)
- MCP server management
- ts-rs auto-generation of TypeScript types for admin API
- Client-contributed admin panels (Lumina serves its own setup UI through daemon port)

---

## Key files changed

| Area | Files |
|------|-------|
| **Config** | `config/src/settings.rs` — `daemon_secret`, `admin_email`, `google_client_id/secret` |
| **Auth types** | `simply-daemon/src/auth.rs` — `RequestUser`, `UserTier` |
| **Auth routes** | `simply-daemon/src/net/auth_routes.rs` — `/auth/login`, `/auth/callback`, `/auth/status` |
| **Admin API** | `simply-daemon/src/net/admin_api.rs` — settings, users, tokens, setup-status |
| **Server** | `simply-daemon/src/net/rest.rs` — middleware, routing, admin file serving |
| **Storage** | `simply-core/src/storage/implementations/sqlite/user.rs` — `user_tokens`, `discord_user_mappings` |
| **Storage** | `simply-core/src/storage/implementations/sqlite/document.rs` — `is_public` column |
| **Documents** | `simply-daemon/src/services.rs` — ownership checks on all document/tab operations |
| **Client** | `simply-daemon/src/net/client.rs` — auth headers on REST + WS |
| **Lumina** | `lumina/src/commands/auth.rs` — `/auth` slash command |
| **Admin UI** | `admin/` — Astro project (layout, pages, API client) |
| **Launch** | `bin/daemon` — builds admin, prints URL |
| **Design** | `docs/designs/ADMIN_UI.md`, `docs/designs/AUTH_AND_IDENTITY.md` |
