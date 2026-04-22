//! Discord-friendly JSON pretty-printing.
//!
//! Like `serde_json::to_string_pretty` but collapses "leaf" arrays (no
//! nested objects, fits on one line) onto a single line so payloads
//! like `[0, "A2", 2]` don't explode into four lines each. Keeps the
//! normal indented layout everywhere else.

use serde_json::Value;

/// Max one-line length for a subtree to be eligible for inlining.
/// Below this threshold the subtree renders as `[a, b, c]` / `{k: v}`;
/// above it falls back to indented multi-line.
const INLINE_WIDTH: usize = 80;

/// Pretty-print `value` with leaf-array compaction.
pub fn pretty_compact(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0);
    out
}

fn indent(level: usize) -> String {
    "  ".repeat(level)
}

fn write_value(out: &mut String, value: &Value, level: usize) {
    if let Some(one_line) = try_inline(value) {
        out.push_str(&one_line);
        return;
    }
    match value {
        Value::Object(map) => write_object(out, map, level),
        Value::Array(arr) => write_array(out, arr, level),
        _ => out.push_str(&value.to_string()),
    }
}

/// Try to render `value` on a single line. Succeeds when the subtree
/// contains no objects (it's "leaf-like") and the rendered length is
/// under `INLINE_WIDTH`.
fn try_inline(value: &Value) -> Option<String> {
    if contains_object(value) {
        return None;
    }
    let s = render_inline(value);
    if s.len() <= INLINE_WIDTH { Some(s) } else { None }
}

fn contains_object(v: &Value) -> bool {
    match v {
        Value::Object(_) => true,
        Value::Array(arr) => arr.iter().any(contains_object),
        _ => false,
    }
}

fn render_inline(v: &Value) -> String {
    match v {
        Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(render_inline).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{}: {}", quote_key(k), render_inline(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        _ => v.to_string(),
    }
}

fn quote_key(k: &str) -> String {
    serde_json::to_string(k).unwrap_or_else(|_| format!("\"{k}\""))
}

fn write_object(out: &mut String, map: &serde_json::Map<String, Value>, level: usize) {
    if map.is_empty() {
        out.push_str("{}");
        return;
    }
    out.push('{');
    out.push('\n');
    let inner = indent(level + 1);
    let outer = indent(level);
    let last = map.len() - 1;
    for (i, (k, v)) in map.iter().enumerate() {
        out.push_str(&inner);
        out.push_str(&quote_key(k));
        out.push_str(": ");
        write_value(out, v, level + 1);
        if i != last { out.push(','); }
        out.push('\n');
    }
    out.push_str(&outer);
    out.push('}');
}

fn write_array(out: &mut String, arr: &[Value], level: usize) {
    if arr.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    out.push('\n');
    let inner = indent(level + 1);
    let outer = indent(level);
    let last = arr.len() - 1;
    for (i, v) in arr.iter().enumerate() {
        out.push_str(&inner);
        write_value(out, v, level + 1);
        if i != last { out.push(','); }
        out.push('\n');
    }
    out.push_str(&outer);
    out.push(']');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn inlines_short_leaf_array() {
        let v = json!([0, "A2", 2]);
        assert_eq!(pretty_compact(&v), "[0, \"A2\", 2]");
    }

    #[test]
    fn inlines_empty_collections() {
        assert_eq!(pretty_compact(&json!([])), "[]");
        assert_eq!(pretty_compact(&json!({})), "{}");
    }

    #[test]
    fn object_stays_indented_even_if_short() {
        let v = json!({ "a": 1, "b": 2 });
        assert!(pretty_compact(&v).contains("\n  \"a\": 1"));
    }

    #[test]
    fn nested_leaf_arrays_inline_inside_outer() {
        let v = json!({
            "notes": [[0, "A2", 2], [2, "G2", 2]]
        });
        let s = pretty_compact(&v);
        assert!(s.contains("[0, \"A2\", 2]"), "got:\n{s}");
        assert!(s.contains("[2, \"G2\", 2]"), "got:\n{s}");
    }

    #[test]
    fn long_array_falls_back_to_multi_line() {
        let v = Value::Array((0..40).map(|i| json!(i)).collect());
        let s = pretty_compact(&v);
        assert!(s.contains('\n'), "long arrays should wrap, got:\n{s}");
    }

    #[test]
    fn array_of_objects_never_inlines() {
        let v = json!([{"a": 1}, {"a": 2}]);
        let s = pretty_compact(&v);
        assert!(s.contains('\n'));
    }
}
