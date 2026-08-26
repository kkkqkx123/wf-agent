use std::collections::HashMap;

use serde_json::Value;

use crate::error::{ExecutionSharedError, ExecutionSharedResult};

pub fn resolve_payload_template(
    payload: &Value,
    context: &HashMap<String, Value>,
) -> ExecutionSharedResult<Value> {
    match payload {
        Value::String(s) => {
            let resolved = resolve_template_string(s, context)?;
            match serde_json::from_str::<Value>(&resolved) {
                Ok(parsed) => Ok(parsed),
                Err(_) => Ok(Value::String(resolved)),
            }
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

fn resolve_template_string(
    template: &str,
    context: &HashMap<String, Value>,
) -> ExecutionSharedResult<String> {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next();
            let mut path = String::new();
            let mut found_close = false;

            while let Some(c) = chars.next() {
                if c == '}' && chars.peek() == Some(&'}') {
                    chars.next();
                    found_close = true;
                    break;
                }
                path.push(c);
            }

            if !found_close {
                return Err(ExecutionSharedError::HookError(format!(
                    "unclosed template expression: {{{{{}",
                    path
                )));
            }

            let path = path.trim();
            let value = resolve_path(path, context).ok_or_else(|| {
                ExecutionSharedError::HookError(format!(
                    "template variable '{}' not found in context",
                    path
                ))
            })?;

            match value {
                Value::String(s) => result.push_str(s),
                other => result.push_str(&other.to_string()),
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

fn resolve_path<'a>(path: &str, context: &'a HashMap<String, Value>) -> Option<&'a Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = context.get(parts[0])?;

    for part in &parts[1..] {
        current = current.get(part)?;
    }

    Some(current)
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
}
