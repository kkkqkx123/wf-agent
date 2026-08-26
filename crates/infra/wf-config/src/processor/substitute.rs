use std::collections::HashMap;
use std::sync::LazyLock;

use crate::error::{ConfigError, ConfigResult};

static PARAM_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\{\{parameters\.([a-zA-Z0-9_.-]+)\}\}").unwrap());

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
}
