# Admin UI & Setup Wizard

**Status:** refined
**Depends on:** Auth & Identity (Stage 1-2, complete)

---

## Problem

Starting the daemon today requires hand-editing `settings.toml` — user email, API keys, model config. There's no guidance for new users. Every client (Noema, Lumina) has its own config file that must be separately maintained.

## Goals

1. **Zero TOML editing** — all configuration through the web UI
2. **Guided setup** — first launch walks you through everything needed to get running
3. **Localhost = admin** — if you're on the machine, you're admin. No login for local use.
4. **Passkeys for remote access** — future: register a passkey on first visit, use it from anywhere
5. **Client-contributed pages** — Lumina (and future clients) register their own setup UI with the daemon
6. **Single admin surface** — daemon, Noema, and Lumina config all managed from one place
7. **No external OAuth in daemon** — external auth (Google, Discord) handled by MCP servers, not the daemon core

## Non-Goals

- Multi-tenant admin (separate admin panels per user)
- Google OAuth built into the daemon (security/GDPR risk, too many dependencies)
- Persistent sessions in the database (in-memory with TTL only)

---

## Design

### Auth Model

**Localhost = admin.** No login needed. The daemon binds to `127.0.0.1`, so only local processes can reach it. The admin page and API are fully accessible from localhost.

**Remote access (future):**
- Phase 1: Passkey registration from localhost, then usable from anywhere
- A console PIN printed on startup as an interim fallback
- Sessions are in-memory with TTL — vanish on restart. No cookies in the database.

**Service clients (Noema, Lumina):**
- Authenticate with `daemon_secret` (Bearer token) — unchanged from current implementation
- `X-User-Id` header for per-user context — only honored from daemon_secret bearers

### Tech Stack

**Astro** frontend in `simply-daemon/admin/`:
- Static site generator — builds to HTML/CSS/JS
- Output served by the daemon from `simply-daemon/admin/dist/`
- Falls back to embedded HTML if build dir not found
- `bin/daemon` runs `npm run build` before starting

### Setup Wizard Flow

The admin page detects setup state via `GET /admin/api/setup-status`. If `user_email` is not configured, it shows the wizard instead of the dashboard.

**Step 1 — Who are you?**
- Enter your email address
- Saved as `user_email` in settings (used for document ownership, default UCM user)

**Step 2 — API Keys**
- Cards for each supported provider: Anthropic, OpenAI, Google (Gemini), Mistral
- Each card: provider name, direct link to their API key console, input field, status
- Skip-able — can always add later from the dashboard

**Step 3 — Done**
- Summary of what's configured
- "Go to Dashboard" button

### Dashboard

- **Connections** — live WS connections (auto-refreshes)
- **Sessions** — active LLM sessions
- **API Keys** — add/remove per provider
- **Users** — UCM user list
- **Settings** — user email, default model

### Admin API Endpoints

```
GET    /admin/api/setup-status     — is setup complete?
GET    /admin/api/settings         — current settings (secrets redacted)
PUT    /admin/api/settings         — update settings
POST   /admin/api/api-key          — set API key for provider
DELETE /admin/api/api-key/{prov}   — remove API key
GET    /admin/api/users            — list users
POST   /admin/api/users            — create user by email
GET    /admin/api/connections      — active WS connections
GET    /auth/status                — auth method info
```

### Client-Contributed Admin Pages (Future)

Clients register their own setup/config pages with the daemon via WS:
- Lumina sends static assets + API route definitions on connect
- Daemon serves at `/admin/clients/{name}/*`
- Main dashboard shows tabs for registered client panels
- Panel lifecycle follows client connection lifecycle

---

## Migration Path

- **Phase 1 (current)**: Admin API + Astro setup wizard + dashboard. Localhost = admin.
- **Phase 2**: Passkey auth for remote access.
- **Phase 3**: Client-contributed panels (Lumina setup UI).
- **Phase 4**: ts-rs auto-generation of TypeScript types for admin API.
