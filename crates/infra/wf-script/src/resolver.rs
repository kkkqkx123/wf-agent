use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::types::ScriptArgument;
use crate::error::{ScriptError, ScriptResult};

/// Matches `$ref.path` style variable references. Pre-compiled singleton so
/// the regex is built once instead of on every `resolve_string` call.
static VAR_REF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$(\w+(?:\.\w+)*)")
        .expect("invariant: regex literal is a fixed pattern and must compile")
});

pub(crate) struct ArgumentResolver;

impl ArgumentResolver {
    pub(crate) fn resolve(
        args: &[ScriptArgument],
        provided_args: &HashMap<String, Value>,
        context_variables: &HashMap<String, Value>,
    ) -> ScriptResult<HashMap<String, Value>> {
        let mut resolved = HashMap::new();

        for arg in args {
            let value = match &arg.source {
                Some(super::types::ArgumentValueSource::Variable) => {
                    context_variables.get(&arg.key).cloned()
                }
                // Static and Expression share the same binding: caller value
                // wins when present, otherwise the declared default is kept.
                // Reference interpolation happens later for every source in
                // the dynamic pass before template render.
                Some(super::types::ArgumentValueSource::Static)
                | Some(super::types::ArgumentValueSource::Expression)
                | None => {
                    if let Some(v) = provided_args.get(&arg.key) {
                        Some(v.clone())
                    } else {
                        arg.default.clone()
                    }
                }
            };

            match value {
                Some(v) => {
                    Self::validate_argument(arg, &v)?;
                    resolved.insert(arg.key.clone(), v);
                }
                None => {
                    if arg.required == Some(true) {
                        return Err(ScriptError::InvalidArgument(format!(
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
        // Type validation
        if let Some(ref arg_type) = arg.r#type {
            match arg_type {
                super::types::ScriptArgumentType::Number => {
                    if !value.is_number() {
                        return Err(ScriptError::InvalidArgument(format!(
                            "Argument '{}' must be a number, got {}",
                            arg.key,
                            json_type_name(value)
                        )));
                    }
                }
                super::types::ScriptArgumentType::Boolean => {
                    if !value.is_boolean() {
                        return Err(ScriptError::InvalidArgument(format!(
                            "Argument '{}' must be a boolean, got {}",
                            arg.key,
                            json_type_name(value)
                        )));
                    }
                }
                super::types::ScriptArgumentType::File
                | super::types::ScriptArgumentType::String => {
                    if !value.is_string() {
                        return Err(ScriptError::InvalidArgument(format!(
                            "Argument '{}' must be a string, got {}",
                            arg.key,
                            json_type_name(value)
                        )));
                    }
                }
            }
        }

        // Options validation - check if value is in allowed options list
        if let Some(options) = &arg.options {
            if !options.is_empty() && !options.contains(value) {
                let options_str: Vec<String> = options
                    .iter()
                    .map(|opt| match opt {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => format!("{:?}", opt),
                    })
                    .collect();
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => format!("{:?}", value),
                };
                return Err(ScriptError::InvalidArgument(format!(
                    "Argument '{}' value '{}' is not in allowed options: [{}]",
                    arg.key,
                    value_str,
                    options_str.join(", ")
                )));
            }
        }

        // Pattern validation - check if string value matches regex pattern
        if let Some(pattern) = &arg.pattern {
            if let Value::String(s) = value {
                match Regex::new(pattern) {
                    Ok(re) => {
                        if !re.is_match(s) {
                            return Err(ScriptError::InvalidArgument(format!(
                                "Argument '{}' value '{}' does not match pattern '{}'",
                                arg.key, s, pattern
                            )));
                        }
                    }
                    Err(e) => {
                        return Err(ScriptError::InvalidDefinition(format!(
                            "Invalid regex pattern '{}' for argument '{}': {}",
                            pattern, arg.key, e
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

/// Validate every file-typed argument value in the resolved map: the value
/// must name an existing regular file inside `workdir` when one is set.
/// Non-file arguments are ignored.
pub(crate) fn validate_file_args(
    args: &[ScriptArgument],
    resolved: &HashMap<String, Value>,
    workdir: Option<&str>,
) -> ScriptResult<()> {
    for arg in args {
        if arg.r#type != Some(super::types::ScriptArgumentType::File) {
            continue;
        }
        let Some(value) = resolved.get(&arg.key) else {
            continue;
        };
        let Value::String(path) = value else {
            continue;
        };
        super::payload::validate_file_arg(path, workdir).map_err(|reason| {
            crate::error::ScriptError::InvalidArgument(format!(
                "Argument '{}' is not a readable input file: {reason}",
                arg.key
            ))
        })?;
    }
    Ok(())
}

pub(crate) struct DynamicResolver;

impl DynamicResolver {
    pub(crate) fn resolve(value: &Value, context: &HashMap<String, Value>) -> Value {
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

    pub(crate) fn resolve_map(
        map: &HashMap<String, Value>,
        context: &HashMap<String, Value>,
    ) -> HashMap<String, Value> {
        map.iter()
            .map(|(k, v)| (k.clone(), Self::resolve(v, context)))
            .collect()
    }

    fn resolve_string(value: &str, context: &HashMap<String, Value>) -> String {
        VAR_REF_RE
            .replace_all(value, |caps: &regex::Captures| {
                let ref_path = caps
                    .get(1)
                    .expect("invariant: capture group 1 is always present for a matched pattern")
                    .as_str();
                match resolve_path(ref_path, context) {
                    Some(resolved) => value_as_string_2(&resolved),
                    None => format!("${}", ref_path),
                }
            })
            .to_string()
    }
}

fn resolve_path(path: &str, context: &HashMap<String, Value>) -> Option<Value> {
    resolve_value_path(path, context)
}

/// Resolve a dotted path against the context. Numeric segments index
/// into arrays, other segments index into objects. Type mismatches and
/// missing keys return `None` so callers report an unresolved placeholder.
pub(crate) fn resolve_value_path(path: &str, context: &HashMap<String, Value>) -> Option<Value> {
    let mut current: Option<&Value> = None;

    for (i, part) in path.split('.').enumerate() {
        if i == 0 {
            current = context.get(part);
        } else {
            match current {
                Some(Value::Object(map)) => {
                    current = map.get(part);
                }
                Some(Value::Array(items)) => {
                    let index: usize = part.parse().ok()?;
                    current = items.get(index);
                }
                _ => return None,
            }
        }
    }

    current.cloned()
}

fn value_as_string_2(value: &Value) -> String {
    value_to_string(value)
}

pub(crate) fn value_to_string(value: &Value) -> String {
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
            label: None,
            required: None,
            default: None,
            source: None,
            description: None,
            options: None,
            pattern: None,
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
            label: None,
            required: None,
            default: Some(json!(8080)),
            source: None,
            description: None,
            options: None,
            pattern: None,
        }];

        let resolved = ArgumentResolver::resolve(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(resolved.get("port"), Some(&json!(8080)));
    }

    #[test]
    fn test_argument_resolve_options_validation() {
        let args = vec![ScriptArgument {
            key: "env".to_string(),
            r#type: Some(crate::ScriptArgumentType::String),
            label: None,
            required: Some(true),
            default: Some(json!("dev")),
            source: None,
            description: None,
            options: Some(vec![json!("dev"), json!("staging"), json!("prod")]),
            pattern: None,
        }];

        // Valid option
        let mut provided = HashMap::new();
        provided.insert("env".to_string(), json!("prod"));
        let resolved = ArgumentResolver::resolve(&args, &provided, &HashMap::new()).unwrap();
        assert_eq!(resolved.get("env"), Some(&json!("prod")));

        // Invalid option
        let mut provided = HashMap::new();
        provided.insert("env".to_string(), json!("invalid"));
        let result = ArgumentResolver::resolve(&args, &provided, &HashMap::new());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("allowed options"));
    }

    #[test]
    fn test_argument_resolve_pattern_validation() {
        let args = vec![ScriptArgument {
            key: "email".to_string(),
            r#type: Some(crate::ScriptArgumentType::String),
            label: None,
            required: Some(true),
            default: None,
            source: None,
            description: None,
            options: None,
            pattern: Some(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string()),
        }];

        // Valid email
        let mut provided = HashMap::new();
        provided.insert("email".to_string(), json!("user@example.com"));
        let resolved = ArgumentResolver::resolve(&args, &provided, &HashMap::new()).unwrap();
        assert_eq!(resolved.get("email"), Some(&json!("user@example.com")));

        // Invalid email
        let mut provided = HashMap::new();
        provided.insert("email".to_string(), json!("not-an-email"));
        let result = ArgumentResolver::resolve(&args, &provided, &HashMap::new());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not match pattern"));
    }

    #[test]
    fn test_argument_resolve_variable_source() {
        use crate::ArgumentValueSource;

        let args = vec![ScriptArgument {
            key: "user".to_string(),
            r#type: None,
            label: None,
            required: None,
            default: None,
            source: Some(ArgumentValueSource::Variable),
            description: None,
            options: None,
            pattern: None,
        }];

        let mut context = HashMap::new();
        context.insert("user".to_string(), json!("alice"));

        let resolved = ArgumentResolver::resolve(&args, &HashMap::new(), &context).unwrap();
        assert_eq!(resolved.get("user"), Some(&json!("alice")));
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

    #[test]
    fn test_expression_source_prefers_provided_value() {
        use crate::ArgumentValueSource;

        let args = vec![ScriptArgument {
            key: "target".to_string(),
            r#type: None,
            label: None,
            required: None,
            default: Some(json!("$fallback.dir")),
            source: Some(ArgumentValueSource::Expression),
            description: None,
            options: None,
            pattern: None,
        }];

        let mut provided = HashMap::new();
        provided.insert("target".to_string(), json!("$primary.dir"));
        let mut context = HashMap::new();
        context.insert("primary".to_string(), json!({"dir": "chosen"}));
        context.insert("fallback".to_string(), json!({"dir": "default"}));

        let resolved = ArgumentResolver::resolve(&args, &provided, &context).unwrap();
        let interpolated = DynamicResolver::resolve_map(&resolved, &context);
        assert_eq!(
            interpolated.get("target"),
            Some(&json!("chosen")),
            "provided value wins and its reference is interpolated"
        );

        let resolved_default = ArgumentResolver::resolve(&args, &HashMap::new(), &context).unwrap();
        let interpolated_default = DynamicResolver::resolve_map(&resolved_default, &context);
        assert_eq!(
            interpolated_default.get("target"),
            Some(&json!("default")),
            "default reference is interpolated when nothing is provided"
        );
    }
}
