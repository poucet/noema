use std::path::PathBuf;
use directories::{BaseDirs, ProjectDirs};

pub struct PathManager;

impl PathManager {
    // Helper to get the base data directory
    fn base_data_dir() -> Option<PathBuf> {
        // On desktop, we use directories::BaseDirs::data_dir() joined with "noema"
        BaseDirs::new().map(|d| d.data_dir().join("noema"))
    }

    pub fn data_dir() -> Option<PathBuf> {
        Self::base_data_dir()
    }

    pub fn config_dir() -> Option<PathBuf> {
        // Match data dir for simplicity unless we really need separate config
        // Or use BaseDirs::config_dir().join("noema")
        BaseDirs::new().map(|d| d.config_dir().join("noema"))
    }

    pub fn cache_dir() -> Option<PathBuf> {
        BaseDirs::new().map(|d| d.cache_dir().join("noema"))
    }

    pub fn db_path() -> Option<PathBuf> {
        Self::data_dir().map(|d| d.join("conversations.db"))
    }

    pub fn logs_dir() -> Option<PathBuf> {
        // On macOS, logs usually go to ~/Library/Logs/
        #[cfg(target_os = "macos")]
        {
            if let Some(dirs) = directories::UserDirs::new() {
                return Some(dirs.home_dir().join("Library/Logs/Noema"));
            }
        }
        // Fallback for other OS
        Self::data_dir().map(|d| d.join("logs"))
    }

    pub fn log_file_path() -> Option<PathBuf> {
        Self::logs_dir().map(|d| d.join("noema.log"))
    }

    pub fn models_dir() -> Option<PathBuf> {
        Self::data_dir().map(|d| d.join("models"))
    }

    pub fn whisper_model_path() -> Option<PathBuf> {
        Self::models_dir().map(|d| d.join("ggml-base.en.bin"))
    }
    
    pub fn ensure_dirs_exist() -> std::io::Result<()> {
        if let Some(d) = Self::data_dir() {
            std::fs::create_dir_all(&d)?;
        }
        if let Some(d) = Self::config_dir() {
            std::fs::create_dir_all(&d)?;
        }
        if let Some(d) = Self::logs_dir() {
            std::fs::create_dir_all(&d)?;
        }
        if let Some(d) = Self::models_dir() {
            std::fs::create_dir_all(&d)?;
        }
        Ok(())
    }
}