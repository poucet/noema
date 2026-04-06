# Admin UI & Setup Wizard

**Status:** draft
**Depends on:** Auth & Identity (Stage 1-2, in progress)

---

## Problem

Starting the daemon today requires hand-editing `settings.toml` — admin email, API keys, model config. There's no guidance for new users. The admin "page" is a bare HTML dashboard with no configuration capabilities. Every client (Noema, Lumina) has its own config file that must be separately maintained.

A new user who starts the daemon has no idea what to do next.

## Goals

1. **Zero TOML editing** — all configuration through the web UI
2. **Guided setup** — first launch walks you through everything needed to get running
3. **Sign in with Google, nothing else** — built-in OAuth credentials for localhost; no client ID/secret setup for local users
4. **Cloud-ready** — cloud deployments override OAuth creds via env vars, same flow after that
5. **Client-contributed pages** — Lumina (and future clients) register their own setup UI with the daemon, served through the daemon's port
6. **Single admin surface** — daemon, Noema, and Lumina config all managed from one place

## Non-Goals

- Multi-tenant admin (separate admin panels per user)
- Mobile-optimized UI (desktop/laptop browser is fine)
- Real-time config sync (page reload after changes is acceptable)

---

## Design

### Tech Stack

**Astro** for the admin frontend:
- Static site generator — builds to plain HTML/CSS/JS
- Island architecture — interactive components only where needed (e.g., API key forms)
- Output embedded in the daemon binary via `include_dir!` or served from a build artifact directory
- No runtime Node.js dependency — it's just static files

**Build integration:**
- `admin/` directory in the repo with Astro project
- `cargo build` triggers `npm run build` in admin/ (build script or build.rs)
- Output goes to `admin/dist/` → embedded or served by axum

### OAuth Bootstrap

**Local mode (default):**
- Built-in Google OAuth client ID + secret compiled into the binary
- Redirect URI: `http://localhost:{port}/auth/callback`
- User sees "Sign in with Google" immediately — no prerequisites

**Cloud mode:**
- Operator sets `GOOGLE_CLIENT_ID` + `GOOGLE_CLIENT_SECRET` env vars
- Redirect URI: configured via `OAUTH_REDIRECT_URI` env var or derived from `Host` header
- Same setup wizard flow after OAuth is configured

**Detection:**
- If env var overrides present → use them (cloud)
- Else → use built-in creds with localhost redirect (local)
- If neither and no built-in → show manual OAuth configuration step (fallback)

### Google OAuth Credential Bootstrap

Three tiers — each user hits the simplest one that works for them:

**Tier 1 — Built-in creds (localhost, zero config):**
- Ship a `SIMPLY_GOOGLE_CLIENT_ID` + `SIMPLY_GOOGLE_CLIENT_SECRET` compiled into the binary
- Redirect URI: `http://localhost:{port}/auth/callback`
- Works immediately for local users — they just click "Sign in with Google"

**Tier 2 — Guided web wizard (cloud / self-hosted):**
When no creds are available and not localhost, the setup wizard shows an interactive Google Cloud walkthrough:
1. "Create a Google Cloud project" — direct link to `console.cloud.google.com/projectcreate`
2. "Configure OAuth branding" — link to `console.cloud.google.com/auth/branding`, set app name to "Simply"
3. "Set audience" — link to `console.cloud.google.com/auth/audience`, choose "External"
4. "Create OAuth 2.0 Client ID" — link to `console.cloud.google.com/apis/credentials/oauthclient`, type: Web application, redirect URI pre-filled from current URL
4. Paste fields for client ID + secret
5. "Test Connection" button — tries a test OAuth redirect to verify creds work
6. On success: saves to settings, proceeds to sign-in

Each step has a "I've done this →" button. The wizard remembers progress so you can close and come back.

**Tier 3 — CLI bootstrap (headless / CI):**
```bash
simply-daemon setup-google-oauth
```
- If `gcloud` CLI is available: automates project creation, API enablement, credential creation
- Consent screen still requires browser — opens the URL and waits
- Prompts for paste of client ID + secret
- Writes to `settings.toml` or stdout for env var piping
- For fully headless: `simply-daemon setup-google-oauth --client-id=X --client-secret=Y`

### Setup Wizard Flow

The admin page detects setup state via `GET /admin/api/setup-status`. If not configured, it shows the wizard instead of the dashboard.

**Step 1 — Sign in with Google (first user becomes admin)**
- If built-in creds available (localhost): "Sign in with Google" button immediately
- If no creds (cloud): Google Cloud setup walkthrough first (see above), then sign-in
- **First sign-in bootstraps the admin:**
  - Google profile email → saved as `admin_email` in settings
  - Same email → saved as `user_email` (used by Noema for document ownership etc.)
  - UCM user record created in the database (`users` table)
  - Admin session cookie issued
- After admin exists: sign-in page still shown, but only the admin email gets dashboard access
- No manual email entry ever — identity comes from Google

**Step 2 — API Keys**
- Cards for each supported provider: Anthropic, OpenAI, Google (Gemini), Mistral, Ollama
- Each card shows:
  - Provider name + logo
  - Direct link to their API key console (e.g., `console.anthropic.com/settings/keys`)
  - Input field for pasting the key
  - Status indicator (configured / not configured / invalid)
  - Brief instructions ("Click the link above, create an API key, paste it here")
- User adds keys as they have them — none are required to proceed
- "Skip for now" option — can always come back from the dashboard
- At least one key recommended before proceeding (show which models become available)

**Step 3 — Done**
- Summary of what's configured
- Which models are available based on configured keys
- "Go to Dashboard" button
- Optionally: "Set up Lumina (Discord bot)" link if Lumina panel is registered

### Dashboard (Post-Setup)

The admin dashboard replaces the current bare HTML page:

- **Status overview** — connected clients, active sessions, daemon health
- **Settings** — admin email, default model, daemon port
- **API Keys** — add/remove/status per provider
- **Users** — list users, view linked accounts, issue/revoke tokens
- **Connections** — live connected clients (WS connections)
- **Client panels** — tabs/links for registered client setup pages (e.g., Lumina)

### Admin API Endpoints

All config management happens through REST. The admin page is just a consumer of these APIs.

```
GET    /admin/api/setup-status          — is setup complete?
GET    /admin/api/settings              — current settings (secrets redacted)
PUT    /admin/api/settings              — update settings fields
POST   /admin/api/settings/api-key      — set API key for provider
DELETE /admin/api/settings/api-key      — remove API key for provider
GET    /admin/api/users                 — list users
POST   /admin/api/users                 — create user by email
POST   /admin/api/users/token           — issue user token
POST   /admin/api/users/revoke-tokens   — revoke all tokens for user
GET    /admin/api/connections           — active WS connections (existing)
GET    /admin/api/client-panels         — list registered client panels
```

**Auth for admin API:**
- During setup (no `admin_email` set): accessible without auth (localhost bootstrap)
- After setup: requires admin session cookie (from Google OAuth) or daemon_secret
- Cloud: requires admin session cookie (no open localhost assumption)

### Client-Contributed Admin Pages

Clients (Lumina, future clients) register their own setup/config pages with the daemon.

**Registration protocol:**
When a client connects via WebSocket, it can register admin panel assets:

```json
{
  "method": "admin.register_panel",
  "params": {
    "client_name": "lumina",
    "display_name": "Discord Bot",
    "routes": [
      { "path": "/admin/clients/lumina/*", "type": "static", "assets": "base64-encoded-tarball" }
    ],
    "api_routes": [
      { "path": "/admin/clients/lumina/api/*", "type": "proxy", "target": "ws" }
    ]
  }
}
```

**How it works:**
1. Client connects to daemon via WS (existing flow)
2. Client sends `admin.register_panel` with its static assets + API route definitions
3. Daemon extracts assets to a temp directory or serves from memory
4. Daemon adds routes: `/admin/clients/{name}/*` → static files, `/admin/clients/{name}/api/*` → proxied to client via WS
5. Main admin dashboard shows "Discord Bot" tab linking to `/admin/clients/lumina/`
6. When client disconnects, panel is unregistered

**Benefits:**
- Daemon doesn't know about Lumina internals
- Lumina owns its own UI and API
- New clients can contribute panels without daemon changes
- Panel lifecycle follows client connection lifecycle

### Lumina Setup Panel (Future — separate design)

Served from Lumina through the client panel system:

- **Discord bot token** — input field + validation (test connection)
- **Invite to server** — generates OAuth2 invite URL with required permissions, user clicks, bot joins, `guild_ids` auto-populated
- **Owner ID** — Discord OAuth to identify the bot owner (one-time)
- **Voice config** — STT/TTS provider selection
- **Status** — connected guilds, registered commands, MCP tools

---

## Open Questions

1. **Astro build integration** — build.rs, Makefile, or manual `npm run build` before `cargo build`? Need to avoid slowing down Rust compilation for non-admin changes.
2. **Asset embedding vs serving** — `include_dir!` (larger binary, simpler deployment) vs serve from filesystem (smaller binary, needs file path)?
3. **Session management** — cookie-based sessions for admin page. JWT or opaque token + server-side store?
4. **Client panel asset size** — how much static content can a client register? Need a size limit to prevent abuse.
5. **Built-in Google OAuth app** — who owns the Google Cloud project? Need to create one for the "simply" project.

## Migration

- **Phase 1**: Admin API endpoints + setup wizard (plain HTML, no Astro yet) — validate the flow
- **Phase 2**: Astro frontend — proper UI with components, styling, interactivity
- **Phase 3**: Client-contributed panels — Lumina registers its setup page
- **Phase 4**: Cloud hardening — session management, HTTPS, CSRF
