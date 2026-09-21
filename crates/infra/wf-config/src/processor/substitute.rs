use std::collections::HashMap;
use std::sync::LazyLock;

use crate::error::{ConfigError, ConfigResult};

static PARAM_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\{\{parameters\.([a-zA-Z0-9_.-]+)\}\}")
        .expect("invariant: regex literal is a fixed pattern and must compile")
});

pub fn substitute_string(input: &str, parameters: &HashMap<String, String>) -> String {
    PARAM_REGEX
        .replace_all(input, |caps: &regex::Captures| {
            let param_name = &caps[1];
            match parameters.get(param_name) {
                Some(value) => value.clone(),
                None => caps[0].to_string(),
            }
        })
        .to_string()
}

pub fn substitute_parameters_in_value(
    value: &mut serde_json::Value,
    parameters: &HashMap<String, String>,
) {
    match value {
        serde_json::Value::String(s) => {
            *s = substitute_string(s, parameters);
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                substitute_parameters_in_value(item, parameters);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                substitute_parameters_in_value(v, parameters);
            }
        }
        _ => {}
    }
}

pub fn substitute_in_struct<T>(
    value: &mut T,
    parameters: &HashMap<String, String>,
) -> ConfigResult<()>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    if parameters.is_empty() {
        return Ok(());
    }
    let mut json_value = serde_json::to_value(&*value).map_err(|e| {
        ConfigError::Serialization(format!("failed to serialize for substitution: {e}"))
    })?;
    substitute_parameters_in_value(&mut json_value, parameters);
    *value = serde_json::from_value(json_value).map_err(|e| {
        ConfigError::Serialization(format!("failed to deserialize after substitution: {e}"))
    })?;
    Ok(())
}

/// Collect `{{parameters.name}}` placeholder names in first-seen order.
pub fn extract_parameter_names(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    for caps in PARAM_REGEX.captures_iter(input) {
        let name = caps
            .get(1)
            .expect("invariant: capture group 1 is always present for a matched pattern")
            .as_str()
            .to_string();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// Placeholders present in the text but absent from the parameter map.
/// The default substitution keeps such spans verbatim, so callers use
/// this query for explicit startup checks without changing that behavior.
pub fn find_unresolved_parameters(
    input: &str,
    parameters: &HashMap<String, String>,
) -> Vec<String> {
    extract_parameter_names(input)
        .into_iter()
        .filter(|name| !parameters.contains_key(name))
        .collect()
}

/// Fail when any `{{parameters.name}}` placeholder lacks a value.
/// Startup paths for key material call this explicitly; the default
/// substitution itself stays lenient so optional placeholders preview
/// verbatim.
pub fn ensure_no_unresolved_parameters_in_value(
    value: &serde_json::Value,
    parameters: &HashMap<String, String>,
) -> ConfigResult<()> {
    let missing = find_unresolved_parameters_in_value(value, parameters);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::Validation(format!(
            "unresolved parameters: [{}]",
            missing.join(", ")
        )))
    }
}

/// Strict substitution for key material: substitutes then fails when any
/// placeholder remains, instead of leaving partial text behind.
pub fn substitute_in_struct_strict<T>(
    value: &mut T,
    parameters: &HashMap<String, String>,
) -> ConfigResult<()>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    substitute_in_struct(value, parameters)?;
    let snapshot = serde_json::to_value(&*value).map_err(|e| {
        ConfigError::Serialization(format!("failed to serialize for substitution: {e}"))
    })?;
    ensure_no_unresolved_parameters_in_value(&snapshot, parameters)
}

/// Unresolved parameter names anywhere inside a JSON value.
pub fn find_unresolved_parameters_in_value(
    value: &serde_json::Value,
    parameters: &HashMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::new();
    collect_unresolved_in_value(value, parameters, &mut out);
    out
}

fn collect_unresolved_in_value(
    value: &serde_json::Value,
    parameters: &HashMap<String, String>,
    out: &mut Vec<String>,
) {
    match value {
        serde_json::Value::String(s) => {
            for name in find_unresolved_parameters(s, parameters) {
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_unresolved_in_value(item, parameters, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_unresolved_in_value(v, parameters, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_string() {
        let mut params = HashMap::new();
        params.insert("user.name".to_string(), "Alice".to_string());
        params.insert("env".to_string(), "prod".to_string());

        assert_eq!(
            substitute_string("Hello {{parameters.user.name}}!", &params),
            "Hello Alice!"
        );
        assert_eq!(
            substitute_string("Env: {{parameters.env}}", &params),
            "Env: prod"
        );
        assert_eq!(
            substitute_string("Missing {{parameters.unknown}}", &params),
            "Missing {{parameters.unknown}}"
        );
    }

    #[test]
    fn test_substitute_in_value() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), "World".to_string());

        let mut value = serde_json::json!({
            "greeting": "Hello {{parameters.name}}!",
            "nested": {
                "msg": "{{parameters.name}} says hi"
            },
            "list": ["{{parameters.name}}", "static"]
        });

        substitute_parameters_in_value(&mut value, &params);

        assert_eq!(value["greeting"], "Hello World!");
        assert_eq!(value["nested"]["msg"], "World says hi");
        assert_eq!(value["list"][0], "World");
        assert_eq!(value["list"][1], "static");
    }

    #[test]
    fn test_substitute_empty_params() {
        let params = HashMap::new();
        let result = substitute_string("Hello {{parameters.name}}!", &params);
        assert_eq!(result, "Hello {{parameters.name}}!");
    }

    #[test]
    fn test_unresolved_parameters_query() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), "World".to_string());
        assert!(find_unresolved_parameters("Hello {{parameters.name}}!", &params).is_empty());
        assert_eq!(
            find_unresolved_parameters("Hi {{parameters.missing}}!", &params),
            vec!["missing".to_string()]
        );
        let value = serde_json::json!({"a": "{{parameters.missing}}", "b": "ok"});
        assert_eq!(
            find_unresolved_parameters_in_value(&value, &params),
            vec!["missing".to_string()]
        );
    }
}
