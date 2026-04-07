# v1.0 Handoff — Multi-user & Admin UI

**Date:** 2026-04-07
**Phase:** multi-user (stage 2)

---

## Auth Model

- **Localhost = admin** — no login needed. If you're on the machine, you have full access.
- **daemon_secret** — auto-generated Bearer token for service clients (Noema, Lumina). Stored in `settings.toml`.
- **X-User-Id** — only honored from daemon_secret bearers (service-to-service trust). Cannot be spoofed.
- **No Google OAuth in daemon** — external auth handled by MCP servers. No cookies in database.
- **In-memory sessions** — `SessionStore` with TTL, vanish on restart.
- **Future: passkeys** — for non-localhost admin access.

## What's Built

### Auth Middleware (rest.rs)
- All routes except admin/auth/static require `Bearer {daemon_secret}`
- Localhost requests to admin pages get `RequestUser::Admin`
- `__user_id` injected into RPC dispatch params from middleware

### Admin UI (simply-daemon/admin/)
- Astro project, builds to `admin/dist/`, served by daemon
- **Setup wizard**: email → API keys → done
- **Dashboard**: connections, sessions, users, API keys, settings
- `bin/daemon` runs `npm install + npm run build` before starting, prints admin URL

### Admin API
- `GET /admin/api/setup-status` — is setup complete?
- `GET/PUT /admin/api/settings` — read/update settings
- `POST /admin/api/api-key` — set API key
- `DELETE /admin/api/api-key/{provider}` — remove API key
- `GET/POST /admin/api/users` — list/create users
- `GET /admin/api/connections` — active WS connections

### Storage
- `discord_user_mappings` table (discord_user_id → UCM user)
- `documents.is_public` column for shared documents
- Document ownership checks on all read/write operations

### Client Wiring
- `DaemonRpcConnection` sends daemon_secret + optional X-User-Id on REST and WS
- `connect_or_host` accepts optional user_id parameter
- Lumina `/auth` slash command generates auth link with discord_id

## What Needs Testing

1. `bin/daemon run` — does it build admin, start, print URL?
2. Open `localhost:9800` — setup wizard or dashboard?
3. Setup wizard — enter email, add API key, complete
4. Dashboard — do connections/sessions/users render?
5. Noema connects — does it still work with auth middleware?
6. API key management — add/remove from dashboard
7. Unauthenticated request to `/session` — 401?

## What's Next

- End-to-end testing of the above
- Passkey registration for remote admin access
- Client-contributed admin panels (Lumina setup UI)
- ts-rs auto-generation of TypeScript types for admin API
- Per-user MCP OAuth (Stage 3)

## Key Files

| Area | Files |
|------|-------|
| **Auth** | `simply-daemon/src/auth.rs`, `simply-daemon/src/net/auth_routes.rs` |
| **Server** | `simply-daemon/src/net/rest.rs` |
| **Admin API** | `simply-daemon/src/net/admin_api.rs` |
| **Admin UI** | `simply-daemon/admin/` (Astro project) |
| **Config** | `config/src/settings.rs` |
| **Storage** | `simply-core/src/storage/implementations/sqlite/user.rs` |
| **Documents** | `simply-daemon/src/services.rs` (ownership checks) |
| **Launch** | `bin/daemon` |
| **Design** | `docs/designs/ADMIN_UI.md` |
