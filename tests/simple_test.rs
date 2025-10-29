use commands::completable;

#[completable]
#[derive(Clone, Debug, PartialEq, Eq)]
enum Simple {
    /// First option
    One,
    /// Second option
    Two,
}

#[test]
fn test_enum_from_str() {
    use std::str::FromStr;

    // Test case-insensitive parsing
    assert_eq!(Simple::from_str("one").unwrap(), Simple::One);
    assert_eq!(Simple::from_str("One").unwrap(), Simple::One);
    assert_eq!(Simple::from_str("ONE").unwrap(), Simple::One);
    assert_eq!(Simple::from_str("two").unwrap(), Simple::Two);

    // Test error
    assert!(Simple::from_str("three").is_err());
}

#[tokio::test]
async fn test_enum_completion() {
    use commands::AsyncCompleter;

    let provider = Simple::One;
    // Context with "o" as the partial being completed
    let ctx = commands::Context::new("/test o", &());

    // Test completion
    let completions = provider.complete(&ctx).await.unwrap();
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].value, "one");
    assert_eq!(completions[0].description, Some("First option".to_string()));

    // Test case-insensitive - need new context with "ON" as partial
    let ctx2 = commands::Context::new("/test ON", &());
    let completions = provider.complete(&ctx2).await.unwrap();
    assert_eq!(completions.len(), 1);

    // Test all - empty partial
    let ctx3 = commands::Context::new("/test ", &());
    let completions = provider.complete(&ctx3).await.unwrap();
    assert_eq!(completions.len(), 2);
}
