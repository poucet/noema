use commands::{commandable, completable, AsyncCompleter, Command, CommandRegistry, Registrable};

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

#[commandable]
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
    let ctx = commands::CompletionContext::new("/test".to_string(), 0, &());

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
    let mut app = TestApp {
        value: String::new(),
    };

    // Use registry - command structs are internal
    let mut registry = CommandRegistry::new();
    TestApp::register(&mut registry);

    let result = registry.execute(&mut app, "/set provider1").await.unwrap();

    match result {
        commands::CommandResult::Success(msg) => {
            assert!(msg.contains("Provider1"));
        }
        _ => panic!("Expected Success result"),
    }

    // Verify value was set
    assert_eq!(app.value, "Provider1");
}

#[tokio::test]
async fn test_command_registry() {
    let mut app = TestApp {
        value: String::new(),
    };

    let mut registry = CommandRegistry::new();
    TestApp::register(&mut registry);

    // Execute set command
    let result = registry.execute(&mut app, "/set provider2").await.unwrap();
    match result {
        commands::CommandResult::Success(msg) => {
            assert!(msg.contains("Provider2"));
        }
        _ => panic!("Expected Success"),
    }

    // Execute get command
    let result = registry.execute(&mut app, "/get").await.unwrap();
    match result {
        commands::CommandResult::Success(value) => {
            assert_eq!(value, "Provider2");
        }
        _ => panic!("Expected Success"),
    }
}

#[tokio::test]
async fn test_automatic_completion() {
    let app = TestApp {
        value: String::new(),
    };

    // Use registry for completion - command structs are internal
    let mut registry = CommandRegistry::new();
    TestApp::register(&mut registry);

    // Test completing the provider argument
    let completions = registry.complete(&app, "/set prov", 9).await.unwrap();

    // Should get completions from TestProvider enum automatically
    assert_eq!(completions.len(), 2);
    assert!(completions.iter().any(|c| c.value == "provider1"));
    assert!(completions.iter().any(|c| c.value == "provider2"));

    // Test case-insensitive
    let completions = registry.complete(&app, "/set PROV", 9).await.unwrap();
    assert_eq!(completions.len(), 2);

    // Test specific match
    let completions = registry.complete(&app, "/set provider1", 14).await.unwrap();
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].value, "provider1");
    assert_eq!(completions[0].description, Some("First provider".to_string()));
}

// Test custom completers with context
struct CompleterTestApp {
    current_provider: Option<TestProvider>,
}

#[commandable]
impl CompleterTestApp {
    #[command(name = "configure", help = "Configure with provider and model")]
    async fn configure(
        &mut self,
        provider: TestProvider,
        model_name: Option<String>
    ) -> Result<String, anyhow::Error> {
        self.current_provider = Some(provider.clone());
        Ok(format!("Configured with {:?} and model {:?}", provider, model_name))
    }

    #[completer(arg = "model_name")]
    async fn complete_model_name(
        &self,
        provider: &TestProvider,
        partial: &str
    ) -> Result<Vec<commands::Completion>, anyhow::Error> {
        // Return different models based on provider
        let models = match provider {
            TestProvider::Provider1 => vec!["model1a", "model1b"],
            TestProvider::Provider2 => vec!["model2a", "model2b"],
        };

        Ok(models
            .into_iter()
            .filter(|m| m.starts_with(partial))
            .map(|m| commands::Completion::simple(m))
            .collect())
    }
}

#[tokio::test]
async fn test_custom_completer_with_context() {
    let app = CompleterTestApp {
        current_provider: None,
    };

    // Use registry
    let mut registry = CommandRegistry::new();
    CompleterTestApp::register(&mut registry);

    // Test that custom completer receives parsed provider argument
    let completions = registry.complete(&app, "/configure provider1 mod", 24).await.unwrap();

    // Should get Provider1's models
    assert_eq!(completions.len(), 2);
    assert!(completions.iter().any(|c| c.value == "model1a"));
    assert!(completions.iter().any(|c| c.value == "model1b"));
}

#[tokio::test]
async fn test_optional_arguments() {
    let mut app = CompleterTestApp {
        current_provider: None,
    };

    // Use registry
    let mut registry = CommandRegistry::new();
    CompleterTestApp::register(&mut registry);

    // Test with just provider (no model)
    let result = registry.execute(&mut app, "/configure provider1").await.unwrap();
    match result {
        commands::CommandResult::Success(msg) => {
            assert!(msg.contains("Provider1"));
            assert!(msg.contains("None"));
        }
        _ => panic!("Expected Success"),
    }

    // Test with provider and model
    let result = registry.execute(&mut app, "/configure provider2 model2a").await.unwrap();
    match result {
        commands::CommandResult::Success(msg) => {
            assert!(msg.contains("Provider2"));
            assert!(msg.contains("model2a"));
        }
        _ => panic!("Expected Success"),
    }
}

#[tokio::test]
async fn test_command_name_completion() {
    let app = TestApp {
        value: String::new(),
    };

    let mut registry = CommandRegistry::new();
    TestApp::register(&mut registry);

    // Test completing command names
    let completions = registry.complete(&app, "/se", 3).await.unwrap();
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].value, "set");

    // Test all commands
    let completions = registry.complete(&app, "/", 1).await.unwrap();
    assert_eq!(completions.len(), 2);
    assert!(completions.iter().any(|c| c.value == "set"));
    assert!(completions.iter().any(|c| c.value == "get"));
}

#[tokio::test]
async fn test_second_argument_completion() {
    let app = CompleterTestApp {
        current_provider: None,
    };

    // Use registry
    let mut registry = CommandRegistry::new();
    CompleterTestApp::register(&mut registry);

    // Completing first argument (provider) - should use enum completion
    let completions = registry.complete(&app, "/configure prov", 15).await.unwrap();
    assert_eq!(completions.len(), 2);
    assert!(completions.iter().any(|c| c.value == "provider1"));

    // Completing second argument (model_name) - should use custom completer
    let completions = registry.complete(&app, "/configure provider1 mod", 24).await.unwrap();
    assert_eq!(completions.len(), 2);
    assert!(completions.iter().any(|c| c.value == "model1a"));
    assert!(completions.iter().any(|c| c.value == "model1b"));
}

// TODO: Stateless function commands
// For now, use commandable on a unit struct for global commands:

struct Global;

#[commandable]
impl Global {
    #[command(name = "version", help = "Show version")]
    async fn show_version(&mut self) -> Result<String, anyhow::Error> {
        Ok("Version 1.0.0".to_string())
    }
}

#[tokio::test]
async fn test_global_command_on_unit_struct() {
    let mut global = Global;

    // Use registry
    let mut registry = CommandRegistry::new();
    Global::register(&mut registry);

    let result = registry.execute(&mut global, "/version").await.unwrap();

    match result {
        commands::CommandResult::Success(msg) => {
            assert!(msg.contains("Version 1.0.0"));
        }
        _ => panic!("Expected Success"),
    }
}

#[tokio::test]
async fn test_register_all_commands_helper() {
    let mut app = TestApp {
        value: String::new(),
    };

    let mut registry = CommandRegistry::new();

    // Use the generated helper - no need to know struct names!
    TestApp::register(&mut registry);

    // Verify commands are registered
    assert_eq!(registry.command_names().len(), 2);
    assert!(registry.command_names().contains(&"set"));
    assert!(registry.command_names().contains(&"get"));

    // Verify they work
    let result = registry.execute(&mut app, "/set provider1").await.unwrap();
    assert!(matches!(result, commands::CommandResult::Success(_)));
}