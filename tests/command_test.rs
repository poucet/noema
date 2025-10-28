use commands::{command, completable, AsyncCompleter, Command, CommandRegistry};
use std::sync::Arc;
use tokio::sync::Mutex;

#[completable]
#[derive(Clone, Debug, PartialEq, Eq)]
enum TestProvider {
    /// First provider
    Provider1,
    /// Second provider
    Provider2,
}

struct TestApp {
    value: String,
}

#[command]
impl TestApp {
    #[command(name = "set", help = "Set a value")]
    async fn set_value(&mut self, provider: TestProvider) -> Result<String, anyhow::Error> {
        self.value = format!("{:?}", provider);
        Ok(format!("Value set to {:?}", provider))
    }

    #[command(name = "get", help = "Get the value")]
    async fn get_value(&mut self) -> Result<String, anyhow::Error> {
        Ok(self.value.clone())
    }
}

#[tokio::test]
async fn test_completable_enum() {
    // Test case-insensitive completion
    let provider = TestProvider::Provider1;
    let ctx = commands::CompletionContext::new("/test".to_string(), 0);

    let completions = provider.complete("prov", &ctx).await.unwrap();
    assert_eq!(completions.len(), 2);
    assert!(completions.iter().any(|c| c.value == "provider1"));
    assert!(completions.iter().any(|c| c.value == "provider2"));

    // Test case-insensitive filtering
    let completions = provider.complete("PROV", &ctx).await.unwrap();
    assert_eq!(completions.len(), 2);

    // Test specific match
    let completions = provider.complete("provider1", &ctx).await.unwrap();
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].value, "provider1");
    assert_eq!(completions[0].description, Some("First provider".to_string()));
}

#[tokio::test]
async fn test_command_execution() {
    let app = Arc::new(Mutex::new(TestApp {
        value: String::new(),
    }));

    let mut set_cmd = set_value(Arc::clone(&app));
    let args = commands::ParsedArgs::new("provider1");

    let result = set_cmd.execute(args).await.unwrap();

    match result {
        commands::CommandResult::Success(msg) => {
            assert!(msg.contains("Provider1"));
        }
        _ => panic!("Expected Success result"),
    }

    // Verify value was set
    let app_locked = app.lock().await;
    assert_eq!(app_locked.value, "Provider1");
}

#[tokio::test]
async fn test_command_registry() {
    let app = Arc::new(Mutex::new(TestApp {
        value: String::new(),
    }));

    let mut registry = CommandRegistry::new();
    registry.register(set_value(Arc::clone(&app)));
    registry.register(get_value(Arc::clone(&app)));

    // Execute set command
    let result = registry.execute("/set provider2").await.unwrap();
    match result {
        commands::CommandResult::Success(msg) => {
            assert!(msg.contains("Provider2"));
        }
        _ => panic!("Expected Success"),
    }

    // Execute get command
    let result = registry.execute("/get").await.unwrap();
    match result {
        commands::CommandResult::Success(value) => {
            assert_eq!(value, "Provider2");
        }
        _ => panic!("Expected Success"),
    }
}

#[tokio::test]
async fn test_automatic_completion() {
    let app = Arc::new(Mutex::new(TestApp {
        value: String::new(),
    }));

    let set_cmd = set_value(Arc::clone(&app));

    // Test completing the provider argument
    let ctx = commands::CompletionContext::new("/set prov".to_string(), 10);
    let completions = set_cmd.complete("prov", &ctx).await.unwrap();

    // Should get completions from TestProvider enum automatically
    assert_eq!(completions.len(), 2);
    assert!(completions.iter().any(|c| c.value == "provider1"));
    assert!(completions.iter().any(|c| c.value == "provider2"));

    // Test case-insensitive
    let completions = set_cmd.complete("PROV", &ctx).await.unwrap();
    assert_eq!(completions.len(), 2);

    // Test specific match
    let completions = set_cmd.complete("provider1", &ctx).await.unwrap();
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].value, "provider1");
    assert_eq!(completions[0].description, Some("First provider".to_string()));
}

