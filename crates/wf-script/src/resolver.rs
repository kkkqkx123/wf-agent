use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;

use super::types::ScriptArgument;
use crate::error::{ScriptError, ScriptResult};

pub struct ArgumentResolver;

impl ArgumentResolver {
    pub fn resolve(
        args: &[ScriptArgument],
        provided_args: &HashMap<String, Value>,
        context_variables: &HashMap<String, Value>,
    ) -> ScriptResult<HashMap<String, Value>> {
        let mut resolved = HashMap::new();

        for arg in args {
            let value = if let Some(v) = provided_args.get(&arg.key) {
                Some(v.clone())
            } else if arg.source.as_deref() == Some("variable") {
                context_variables.get(&arg.key).cloned()
            } else {
                arg.default.clone()
            };

            match value {
                Some(v) => {
                    Self::validate_argument(arg, &v)?;
                    resolved.insert(arg.key.clone(), v);
                }
                None => {
                    if arg.required == Some(true) {
                        return Err(ScriptError::Internal(format!(
                            "Required argument '{}' is not provided and has no default",
                            arg.key
                        )));
                    }
                }
            }
        }

        Ok(resolved)
    }

    fn validate_argument(arg: &ScriptArgument, value: &Value) -> ScriptResult<()> {
        if let Some(ref arg_type) = arg.r#type {
            match arg_type {
                super::types::ScriptArgumentType::Number => {
                    if !value.is_number() {
                        return Err(ScriptError::Internal(format!(
                            "Argument '{}' must be a number, got {}",
                            arg.key,
                            json_type_name(value)
                        )));
                    }
                }
                super::types::ScriptArgumentType::Boolean => {
                    if !value.is_boolean() {
                        return Err(ScriptError::Internal(format!(
                            "Argument '{}' must be a boolean, got {}",
                            arg.key,
                            json_type_name(value)
                        )));
                    }
                }
                super::types::ScriptArgumentType::File
                | super::types::ScriptArgumentType::String => {
                    if !value.is_string() {
                        return Err(ScriptError::Internal(format!(
                            "Argument '{}' must be a string, got {}",
                            arg.key,
                            json_type_name(value)
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

fn json_type_name(value: &Value) -> &str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub struct DynamicResolver;

impl DynamicResolver {
    pub fn resolve(value: &Value, context: &HashMap<String, Value>) -> Value {
        match value {
            Value::String(s) => {
                let resolved = Self::resolve_string(s, context);
                Value::String(resolved)
            }
            Value::Array(arr) => {
                Value::Array(arr.iter().map(|v| Self::resolve(v, context)).collect())
            }
            Value::Object(obj) => {
                let mut resolved = serde_json::Map::new();
                for (k, v) in obj {
                    resolved.insert(k.clone(), Self::resolve(v, context));
                }
                Value::Object(resolved)
            }
            other => other.clone(),
        }
    }

    pub fn resolve_map(
        map: &HashMap<String, Value>,
        context: &HashMap<String, Value>,
    ) -> HashMap<String, Value> {
        map.iter()
            .map(|(k, v)| (k.clone(), Self::resolve(v, context)))
            .collect()
    }

    fn resolve_string(value: &str, context: &HashMap<String, Value>) -> String {
        let re = Regex::new(r"\$(\w+(?:\.\w+)*)").unwrap();
        re.replace_all(value, |caps: &regex::Captures| {
            let ref_path = caps.get(1).unwrap().as_str();
            match resolve_path(ref_path, context) {
                Some(resolved) => value_as_string_2(&resolved),
                None => format!("${}", ref_path),
            }
        })
        .to_string()
    }
}

fn resolve_path(path: &str, context: &HashMap<String, Value>) -> Option<Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current: Option<&Value> = None;

    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            current = context.get(*part);
        } else {
            match current {
                Some(Value::Object(map)) => {
                    current = map.get(*part);
                }
                _ => return None,
            }
        }
    }

    current.cloned()
}

fn value_as_string_2(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_argument_resolve_direct() {
        let args = vec![ScriptArgument {
            key: "name".to_string(),
            r#type: None,
            required: None,
            default: None,
            source: None,
            description: None,
        }];
        let mut provided = HashMap::new();
        provided.insert("name".to_string(), json!("world"));

        let resolved = ArgumentResolver::resolve(&args, &provided, &HashMap::new()).unwrap();
        assert_eq!(resolved.get("name"), Some(&json!("world")));
    }

    #[test]
    fn test_argument_resolve_default() {
        let args = vec![ScriptArgument {
            key: "port".to_string(),
            r#type: Some(crate::ScriptArgumentType::Number),
            required: None,
            default: Some(json!(8080)),
            source: None,
            description: None,
        }];

        let resolved = ArgumentResolver::resolve(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(resolved.get("port"), Some(&json!(8080)));
    }

    #[test]
    fn test_dynamic_resolve_string() {
        let mut context = HashMap::new();
        context.insert("user".to_string(), json!("alice"));

        let result = DynamicResolver::resolve_string("Hello $user", &context);
        assert_eq!(result, "Hello alice");
    }

    #[test]
    fn test_dynamic_resolve_dotted_path() {
        let mut context = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("name".to_string(), json!("bob"));
        context.insert("data".to_string(), json!(inner));

        let result = DynamicResolver::resolve_string("User: $data.name", &context);
        assert_eq!(result, "User: bob");
    }

    #[test]
    fn test_dynamic_resolve_unresolved() {
        let context = HashMap::new();
        let result = DynamicResolver::resolve_string("Hello $unknown", &context);
        assert_eq!(result, "Hello $unknown");
    }
}
