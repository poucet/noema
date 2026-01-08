// Tests for the #[tool_methods] macro which enables #[tool] on methods.
//
// This uses the tool_methods macro which processes an entire impl block
// and generates Args structs at module level.

use llm_macros::{tool_methods};

struct Calculator {
    base_value: i32,
}

#[tool_methods]
impl Calculator {
    // Regular method - not a tool
    fn new(base_value: i32) -> Self {
        Self { base_value }
    }

    /// Adds a number to the base value.
    #[tool]
    fn add(&self, amount: i32) -> i32 {
        self.base_value + amount
    }

    #[tool]
    fn scale(&self, factor: i32) -> i32 {
        self.base_value * factor
    }

    #[tool]
    async fn async_compute(&self, x: i32, y: i32) -> i32 {
        self.base_value + x + y
    }

    #[tool]
    fn complex_operation(&self, a: i32, b: String, c: Vec<i32>) -> String {
        format!("base={}, a={}, b={}, c={:?}", self.base_value, a, b, c)
    }
}

#[test]
fn test_method_struct_generated() {
    let args = AddArgs { amount: 10 };
    assert_eq!(args.amount, 10);
}

#[test]
fn test_method_tool_def() {
    let tool_def = AddArgs::add_tool_def();
    assert_eq!(tool_def.name, "add");
    assert!(tool_def.description.is_some());
    assert!(tool_def.description.unwrap().contains("Adds a number"));
}

#[test]

fn test_method_wrapper_sync() {
    let calc = Calculator::new(100);
    let args = AddArgs { amount: 50 };
    let args_json = serde_json::to_value(&args).unwrap();

    // Expected API: wrapper takes &self (the tool context/struct instance)
    let result = args.add_wrapper(&calc, args_json);
    assert!(result.is_ok());
    let result_str = result.unwrap();
    assert_eq!(result_str, "150");
}

#[test]

fn test_method_scale() {
    let calc = Calculator::new(10);
    let args = ScaleArgs { factor: 5 };
    let args_json = serde_json::to_value(&args).unwrap();

    let result = args.scale_wrapper(&calc, args_json);
    assert!(result.is_ok());
    let result_str = result.unwrap();
    assert_eq!(result_str, "50");
}

#[test]

fn test_async_method_wrapper() {
    let calc = Calculator::new(10);
    let args = AsyncComputeArgs { x: 5, y: 3 };
    let args_json = serde_json::to_value(&args).unwrap();

    let result = tokio_test::block_on(async {
        args.async_compute_wrapper(&calc, args_json).await
    });

    assert!(result.is_ok());
    let result_str = result.unwrap();
    assert_eq!(result_str, "18"); // 10 + 5 + 3
}

#[test]

fn test_method_complex_params() {
    let calc = Calculator::new(100);
    let args_json = serde_json::json!({
        "a": 42,
        "b": "test",
        "c": [1, 2, 3]
    });

    let args: ComplexOperationArgs = serde_json::from_value(args_json.clone()).unwrap();
    let result = args.complex_operation_wrapper(&calc, args_json);

    assert!(result.is_ok());
    let result_str = result.unwrap();
    assert!(result_str.contains("base=100"));
    assert!(result_str.contains("a=42"));
    assert!(result_str.contains("b=test"));
}

// Test with multiple structs having methods
struct StringProcessor {
    prefix: String,
}

#[tool_methods]
impl StringProcessor {
    fn new(prefix: String) -> Self {
        Self { prefix }
    }

    #[tool]
    fn process(&self, text: String) -> String {
        format!("{}: {}", self.prefix, text)
    }

    #[tool]
    fn join_items(&self, items: Vec<String>, separator: String) -> String {
        let joined = items.join(&separator);
        format!("{}: {}", self.prefix, joined)
    }
}

#[test]

fn test_different_struct_method() {
    let processor = StringProcessor::new("PREFIX".to_string());
    let args = ProcessArgs {
        text: "hello world".to_string(),
    };
    let args_json = serde_json::to_value(&args).unwrap();

    let result = args.process_wrapper(&processor, args_json);
    assert!(result.is_ok());
    let result_str = result.unwrap();
    assert!(result_str.contains("PREFIX: hello world"));
}

#[test]

fn test_method_multiple_params() {
    let processor = StringProcessor::new("OUTPUT".to_string());
    let args_json = serde_json::json!({
        "items": ["a", "b", "c"],
        "separator": "-"
    });

    let args: JoinItemsArgs = serde_json::from_value(args_json.clone()).unwrap();
    let result = args.join_items_wrapper(&processor, args_json);

    assert!(result.is_ok());
    let result_str = result.unwrap();
    assert!(result_str.contains("OUTPUT: a-b-c"));
}

#[test]

fn test_method_tool_defs_separate() {
    let add_def = AddArgs::add_tool_def();
    let scale_def = ScaleArgs::scale_tool_def();

    assert_eq!(add_def.name, "add");
    assert_eq!(scale_def.name, "scale");
    assert_ne!(add_def.name, scale_def.name);
}

// Test with Result return type
#[tool_methods]
impl Calculator {
    #[tool]
    fn divide(&self, divisor: i32) -> Result<i32, String> {
        if divisor == 0 {
            Err("Division by zero".to_string())
        } else {
            Ok(self.base_value / divisor)
        }
    }
}

#[test]

fn test_method_result_success() {
    let calc = Calculator::new(100);
    let args = DivideArgs { divisor: 5 };
    let args_json = serde_json::to_value(&args).unwrap();

    let result = args.divide_wrapper(&calc, args_json);
    assert!(result.is_ok());
}

#[test]

fn test_method_result_error() {
    let calc = Calculator::new(100);
    let args = DivideArgs { divisor: 0 };
    let args_json = serde_json::to_value(&args).unwrap();

    let result = args.divide_wrapper(&calc, args_json);
    // The wrapper should succeed even if the inner function returns Err
    // because it serializes the Result
    assert!(result.is_ok());
}

#[test]

fn test_method_args_serialization() {
    let args = AddArgs { amount: 42 };
    let json = serde_json::to_string(&args).unwrap();
    assert!(json.contains("42"));

    let deserialized: AddArgs = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.amount, 42);
}
