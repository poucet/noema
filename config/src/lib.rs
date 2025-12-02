pub mod paths;

pub use paths::PathManager;

/// Load environment variables from .env files.
/// First loads from ~/.env (home directory), then from ./.env (project directory).
/// Project directory values take precedence over home directory values.
/// Call this before parsing CLI args to ensure env vars are available.
pub fn load_env_file() {
    // Load from home directory first (lower precedence)
    if let Some(home) = dirs::home_dir() {
        let home_env_path = home.join(".env");
        dotenv::from_path(home_env_path).ok();
    }

    // Load from project directory (higher precedence - overwrites home values)
    // dotenv::dotenv() loads from current directory's .env
    dotenv::dotenv().ok();
}