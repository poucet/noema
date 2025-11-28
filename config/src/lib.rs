use llm::providers::{
    ClaudeProvider, GeminiProvider, GeneralModelProvider, OllamaProvider, OpenAIProvider,
};

/// Load environment variables from .env files.
/// First loads from ~/.env (home directory), then from ./.env (project directory).
/// Project directory values take precedence over home directory values.
/// Call this before parsing CLI args to ensure env vars are available.
pub fn load_env_file() {
    // Load from home directory first (lower precedence)
    if let Some(home) = directories::UserDirs::new() {
        let home_env_path = home.home_dir().join(".env");
        dotenv::from_path(home_env_path).ok();
    }

    // Load from project directory (higher precedence - overwrites home values)
    // dotenv::dotenv() loads from current directory's .env
    dotenv::dotenv().ok();
}

/// Get an API key from environment, panicking if not set.
pub fn get_api_key(key: &str) -> String {
    std::env::var(key).expect(&format!("{} must be set in .env file", key))
}

/// Get an optional environment variable.
pub fn get_env_var(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// Provider URL configuration for proxying requests.
#[derive(Clone, Default, Debug)]
pub struct ProviderUrls {
    pub openai: Option<String>,
    pub claude: Option<String>,
    pub gemini: Option<String>,
    pub ollama: Option<String>,
}

impl ProviderUrls {
    /// Create ProviderUrls from environment variables.
    pub fn from_env() -> Self {
        Self {
            openai: get_env_var("OPENAI_BASE_URL"),
            claude: get_env_var("CLAUDE_BASE_URL"),
            gemini: get_env_var("GEMINI_BASE_URL"),
            ollama: get_env_var("OLLAMA_BASE_URL"),
        }
    }
}

/// Model provider types supported by the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelProviderType {
    Ollama,
    Gemini,
    Claude,
    OpenAI,
}

impl std::fmt::Display for ModelProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::str::FromStr for ModelProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(ModelProviderType::Ollama),
            "gemini" => Ok(ModelProviderType::Gemini),
            "claude" => Ok(ModelProviderType::Claude),
            "openai" => Ok(ModelProviderType::OpenAI),
            _ => Err(format!("Unknown provider: {}", s)),
        }
    }
}

/// Get model ID and display name for a provider type.
pub fn get_model_info(provider_type: &ModelProviderType) -> (&'static str, &'static str) {
    match provider_type {
        ModelProviderType::Ollama => ("gemma3n:latest", "gemma3n:latest"),
        ModelProviderType::Gemini => ("models/gemini-2.5-flash", "gemini-2.5-flash"),
        ModelProviderType::Claude => ("claude-sonnet-4-5-20250929", "claude-sonnet-4-5"),
        ModelProviderType::OpenAI => ("gpt-4o-mini", "gpt-4o-mini"),
    }
}

/// Create a provider instance with optional custom URLs.
pub fn create_provider(
    provider_type: &ModelProviderType,
    urls: &ProviderUrls,
) -> GeneralModelProvider {
    match provider_type {
        ModelProviderType::Ollama => {
            let provider = match &urls.ollama {
                Some(url) => OllamaProvider::new(url),
                None => OllamaProvider::default(),
            };
            GeneralModelProvider::Ollama(provider)
        }
        ModelProviderType::Gemini => {
            let api_key = get_api_key("GEMINI_API_KEY");
            let provider = match &urls.gemini {
                Some(url) => GeminiProvider::new(url, &api_key),
                None => GeminiProvider::default(&api_key),
            };
            GeneralModelProvider::Gemini(provider)
        }
        ModelProviderType::Claude => {
            let api_key = get_api_key("CLAUDE_API_KEY");
            let provider = match &urls.claude {
                Some(url) => ClaudeProvider::new(url, &api_key),
                None => ClaudeProvider::default(&api_key),
            };
            GeneralModelProvider::Claude(provider)
        }
        ModelProviderType::OpenAI => {
            let api_key = get_api_key("OPENAI_API_KEY");
            let provider = match &urls.openai {
                Some(url) => OpenAIProvider::new(url, &api_key),
                None => OpenAIProvider::default(&api_key),
            };
            GeneralModelProvider::OpenAI(provider)
        }
    }
}
