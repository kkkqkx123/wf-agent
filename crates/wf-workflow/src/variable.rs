use dashmap::DashMap;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::WorkflowResult;

pub type VariableStore = Arc<DashMap<String, Value>>;

pub fn create_variable_store() -> VariableStore {
    Arc::new(DashMap::new())
}

pub struct VariableResolver;

impl VariableResolver {
    pub fn resolve(input: &Value, variables: &VariableStore) -> Value {
        match input {
            Value::String(s) => Self::resolve_str(s, variables),
            Value::Object(map) => {
                let resolved: serde_json::Map<String, Value> = map.iter()
                    .map(|(k, v)| (k.clone(), Self::resolve(v, variables)))
                    .collect();
                Value::Object(resolved)
            }
            Value::Array(arr) => {
                let resolved: Vec<Value> = arr.iter()
                    .map(|v| Self::resolve(v, variables))
                    .collect();
                Value::Array(resolved)
            }
            other => other.clone(),
        }
    }

    pub fn resolve_str(input: &str, variables: &VariableStore) -> Value {
        if input.starts_with("${") && input.ends_with("}") {
            let var_name = &input[2..input.len() - 1];
            if let Some(v) = Self::lookup_variable(var_name, variables) {
                return v;
            }
        }

        let mut result = input.to_string();
        let mut start = 0;

        while let Some(pos) = result[start..].find("${") {
            let abs_pos = start + pos;
            if let Some(end) = result[abs_pos..].find('}') {
                let abs_end = abs_pos + end;
                let var_name = &result[abs_pos + 2..abs_end];
                if let Some(v) = Self::lookup_variable(var_name, variables) {
                    let replacement = match &v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    result.replace_range(abs_pos..=abs_end, &replacement);
                    start = abs_pos + replacement.len();
                } else {
                    start = abs_end + 1;
                }
            } else {
                break;
            }
        }

        Value::String(result)
    }

    fn lookup_variable(path: &str, variables: &VariableStore) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let first = parts.first()?;

        let mut current = variables.get(*first)?.clone();

        for part in &parts[1..] {
            if let Value::Object(map) = &current {
                current = serde_json::from_value(serde_json::to_value(map).ok()?).ok()?;
                if let Value::Object(map) = &current {
                    current = map.get(*part)?.clone();
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }

        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(vars: &[(&str, Value)]) -> VariableStore {
        let s = create_variable_store();
        for (k, v) in vars {
            s.insert(k.to_string(), v.clone());
        }
        s
    }

    #[test]
    fn test_resolve_simple() {
        let vars = store(&[("name", Value::String("world".to_string()))]);
        let result = VariableResolver::resolve_str("hello ${name}", &vars);
        assert_eq!(result, Value::String("hello world".to_string()));
    }

    #[test]
    fn test_resolve_missing() {
        let vars = store(&[]);
        let result = VariableResolver::resolve_str("hello ${missing}", &vars);
        assert_eq!(result, Value::String("hello ${missing}".to_string()));
    }

    #[test]
    fn test_resolve_multiple() {
        let vars = store(&[
            ("a", Value::String("A".to_string())),
            ("b", Value::String("B".to_string())),
        ]);
        let result = VariableResolver::resolve_str("${a} and ${b}", &vars);
        assert_eq!(result, Value::String("A and B".to_string()));
    }
}
