//! Strict hook payload templates: every `{{path}}` must resolve against
//! the context, otherwise rendering fails and the hook never fires with a
//! half-substituted payload.
//!
//! Strict failure is the correct semantic here: an unresolved command or
//! payload must never execute with placeholder text left in. The lenient
//! prompt-text renderer (`wf_common::template`, shared by resource
//! templates, approval messages and skill content) keeps unknown
//! placeholders verbatim instead; pick that engine for display text and
//! this one for payloads that execute.
//!
//! The dotted-path lookup delegates to the shared foundation resolver so
//! hook payloads and script arguments resolve object fields and numeric
//! array indices identically. Value flows still differ (direct context
//! table here, declared arguments plus file confinement in `wf-script`),
//! so each engine keeps its own strict failure policy.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::{ExecutionSharedError, ExecutionSharedResult};

pub fn resolve_payload_template(
    payload: &Value,
    context: &HashMap<String, Value>,
) -> ExecutionSharedResult<Value> {
    match payload {
        Value::String(s) => {
            if let Some(preserved) = resolve_single_placeholder_value(s, context)? {
                return Ok(preserved);
            }
            Ok(Value::String(resolve_template_string(s, context)?))
        }
        Value::Object(map) => {
            let mut result = serde_json::Map::new();
            for (k, v) in map {
                result.insert(k.clone(), resolve_payload_template(v, context)?);
            }
            Ok(Value::Object(result))
        }
        Value::Array(arr) => {
            let mut result = Vec::with_capacity(arr.len());
            for v in arr {
                result.push(resolve_payload_template(v, context)?);
            }
            Ok(Value::Array(result))
        }
        other => Ok(other.clone()),
    }
}

/// Whole-string single-placeholder passthrough: the exact text `{{path}}`
/// with no surrounding content preserves the resolved value type instead of
/// coercing through display text. Embedded or multi-placeholder strings
/// always render to text. Returns `None` when the payload is not a single
/// placeholder; failures inside a single placeholder are errors.
fn resolve_single_placeholder_value(
    template: &str,
    context: &HashMap<String, Value>,
) -> ExecutionSharedResult<Option<Value>> {
    let spans = wf_common::template::scan_template_spans(template);
    if spans.len() != 1 {
        return Ok(None);
    }
    let span = &spans[0];
    if !span.closed || span.start != 0 || span.end != template.len() {
        return Ok(None);
    }
    let path = span.name.as_str();
    if let Some(reason) = wf_common::template::validate_template_path(path) {
        return Err(ExecutionSharedError::HookError(format!(
            "template variable '{path}' is invalid: {reason}"
        )));
    }
    let value = resolve_path(path, context).cloned().ok_or_else(|| {
        ExecutionSharedError::HookError(format!("template variable '{path}' not found in context"))
    })?;
    Ok(Some(value))
}

fn resolve_template_string(
    template: &str,
    context: &HashMap<String, Value>,
) -> ExecutionSharedResult<String> {
    let spans = wf_common::template::scan_template_spans(template);
    if spans.is_empty() {
        if template.contains("{{") {
            for span in wf_common::template::scan_template_spans(template) {
                if !span.closed {
                    return Err(ExecutionSharedError::HookError(format!(
                        "unclosed template expression: {{{{{}}}",
                        span.name
                    )));
                }
            }
        }
        return Ok(template.to_string());
    }
    let mut result = String::with_capacity(template.len());
    let mut cursor = 0;
    for span in spans {
        result.push_str(&template[cursor..span.start]);
        if !span.closed {
            return Err(ExecutionSharedError::HookError(format!(
                "unclosed template expression: {{{{{}}}",
                span.name
            )));
        }
        let path = span.name.as_str();
        if let Some(reason) = wf_common::template::validate_template_path(path) {
            return Err(ExecutionSharedError::HookError(format!(
                "template variable '{path}' is invalid: {reason}"
            )));
        }
        let value = resolve_path(path, context).ok_or_else(|| {
            ExecutionSharedError::HookError(format!(
                "template variable '{}' not found in context",
                path
            ))
        })?;
        result.push_str(&wf_common::template::value_to_display_string(value));
        cursor = span.end;
    }
    result.push_str(&template[cursor..]);
    Ok(result)
}

fn resolve_path<'a>(path: &str, context: &'a HashMap<String, Value>) -> Option<&'a Value> {
    wf_common::template::resolve_value_path_ref(path, context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_variable() {
        let mut ctx = HashMap::new();
        ctx.insert("name".to_string(), Value::String("world".to_string()));

        let payload = Value::String("hello {{name}}".to_string());
        let result = resolve_payload_template(&payload, &ctx).unwrap();
        assert_eq!(result, Value::String("hello world".to_string()));
    }

    #[test]
    fn test_nested_path() {
        let mut inner = serde_json::Map::new();
        inner.insert("text".to_string(), Value::String("hello".to_string()));
        let mut ctx = HashMap::new();
        ctx.insert("input".to_string(), Value::Object(inner));

        let payload = Value::String("{{input.text}}".to_string());
        let result = resolve_payload_template(&payload, &ctx).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_object_template() {
        let mut ctx = HashMap::new();
        ctx.insert("val".to_string(), Value::String("replaced".to_string()));

        let mut payload = serde_json::Map::new();
        payload.insert("key".to_string(), Value::String("{{val}}".to_string()));
        payload.insert("num".to_string(), Value::Number(42.into()));

        let result = resolve_payload_template(&Value::Object(payload), &ctx).unwrap();
        match result {
            Value::Object(map) => {
                assert_eq!(
                    map.get("key").unwrap(),
                    &Value::String("replaced".to_string())
                );
                assert_eq!(map.get("num").unwrap(), &Value::Number(42.into()));
            }
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn test_array_template() {
        let mut ctx = HashMap::new();
        ctx.insert("x".to_string(), Value::String("a".to_string()));

        let payload = Value::Array(vec![
            Value::String("{{x}}".to_string()),
            Value::String("static".to_string()),
        ]);
        let result = resolve_payload_template(&payload, &ctx).unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::String("a".to_string()),
                Value::String("static".to_string()),
            ])
        );
    }

    #[test]
    fn test_no_template() {
        let ctx = HashMap::new();
        let payload = Value::String("no templates here".to_string());
        let result = resolve_payload_template(&payload, &ctx).unwrap();
        assert_eq!(result, Value::String("no templates here".to_string()));
    }

    #[test]
    fn test_unclosed_template() {
        let ctx = HashMap::new();
        let payload = Value::String("{{unclosed".to_string());
        let result = resolve_payload_template(&payload, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_variable() {
        let ctx = HashMap::new();
        let payload = Value::String("{{missing}}".to_string());
        let result = resolve_payload_template(&payload, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_numeric_value() {
        let mut ctx = HashMap::new();
        ctx.insert("count".to_string(), Value::Number(42.into()));

        let payload = Value::String("count={{count}}".to_string());
        let result = resolve_payload_template(&payload, &ctx).unwrap();
        assert_eq!(result, Value::String("count=42".to_string()));
    }

    #[test]
    fn test_array_index_path() {
        let mut ctx = HashMap::new();
        ctx.insert("items".to_string(), serde_json::json!(["a", "b"]));

        let payload = Value::String("second={{items.1}}".to_string());
        let result = resolve_payload_template(&payload, &ctx).unwrap();
        assert_eq!(result, Value::String("second=b".to_string()));
    }
}
