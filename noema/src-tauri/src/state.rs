//! Application state for the Tauri shell.
//!
//! With the admin UI driving all daemon interactions over HTTP+WS, Tauri only
//! needs to hold: the daemon (for deep-link OAuth forwarding), the admin
//! RequestContext (for the same), and the REST base URL (exposed to the
//! webview via `daemon_base_url`).

use simply_daemon::api::Daemon;
use simply_daemon::net::DaemonHandle;
use simply_rpc::RequestContext;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::OnceCell;

pub struct AppState {
    /// Guards against concurrent init calls.
    pub initializing: AtomicBool,
    /// The daemon — primary API (embedded or remote).
    pub daemon: OnceCell<Arc<dyn Daemon>>,
    /// Admin user's request context — resolved during init.
    pub admin_ctx: OnceCell<RequestContext>,
    /// Keeps the daemon handle alive (owns server if we're the host).
    pub _daemon_handle: OnceCell<DaemonHandle>,
    /// REST base URL for the daemon (e.g. `http://127.0.0.1:9800`).
    /// Exposed to the webview via `daemon_base_url`.
    pub rest_base_url: OnceCell<String>,
    /// HTTP client with auth headers — kept for any remaining server-side REST
    /// calls in Tauri handlers (e.g. deep-link flows).
    pub http_client: OnceCell<reqwest::Client>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            initializing: AtomicBool::new(false),
            daemon: OnceCell::new(),
            admin_ctx: OnceCell::new(),
            _daemon_handle: OnceCell::new(),
            rest_base_url: OnceCell::new(),
            http_client: OnceCell::new(),
        }
    }

    pub fn get_daemon(&self) -> Result<Arc<dyn Daemon>, String> {
        self.daemon.get().cloned().ok_or_else(|| "Daemon not initialized".to_string())
    }

    pub fn ctx(&self) -> RequestContext {
        self.admin_ctx.get().cloned().unwrap_or_default()
    }

    pub fn is_initialized(&self) -> bool {
        self.daemon.get().is_some()
    }
}

impl Default for AppState {
    fn default() -> Self { Self::new() }
}
