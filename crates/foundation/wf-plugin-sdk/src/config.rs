//! Minimal config validation against a plugin's declared `config_schema`.
//!
//! Supports a pragmatic subset of JSON Schema Draft 2020-12: a top-level
//! `type: "object"`, a `required` array of property names, and per-property
//! `type` strings (`string` / `number` / `integer` / `boolean` / `array` /
//! `object` / `null`). Anything the subset does not express is ignored —
//! validation narrows only when the schema narrows.

use serde_json::Value;

use crate::error::{PluginError, PluginResult};

/// Validate `config` against `schema`. A `None` schema accepts everything.
pub fn validate_config(config: &Value, schema: Option<&Value>) -> PluginResult<()> {
    let Some(schema) = schema else {
        return Ok(());
    };
    let Some(obj) = config.as_object() else {
        return Err(PluginError::ConfigInvalid {
            plugin_id: String::new(),
            reason: "config must be a JSON object".into(),
        });
    };

    // Top-level type narrowing: only "object" schemas constrain here.
    if schema.get("type").and_then(Value::as_str) == Some("object") {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !obj.contains_key(key) {
                    return Err(PluginError::ConfigInvalid {
                        plugin_id: String::new(),
                        reason: format!("missing required config key '{}'", key),
                    });
                }
            }
        }
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            for (key, prop_schema) in properties {
                let Some(value) = obj.get(key) else {
                    continue;
                };
                let Some(expected) = prop_schema.get("type").and_then(Value::as_str) else {
                    continue;
                };
                if !type_matches(value, expected) {
                    return Err(PluginError::ConfigInvalid {
                        plugin_id: String::new(),
                        reason: format!(
                            "config key '{}' expected type '{}', got {}",
                            key,
                            expected,
                            json_type_name(value)
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Validate with the owning plugin id filled into any error.
pub fn validate_config_for(
    plugin_id: &str,
    config: &Value,
    schema: Option<&Value>,
) -> PluginResult<()> {
    validate_config(config, schema).map_err(|e| match e {
        PluginError::ConfigInvalid { reason, .. } => PluginError::ConfigInvalid {
            plugin_id: plugin_id.to_owned(),
            reason,
        },
        other => other,
    })
}

fn type_matches(value: &Value, expected: &str) -> bool {
    match expected {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "required": ["model"],
            "properties": {
                "model": { "type": "string" },
                "temperature": { "type": "number" },
                "retries": { "type": "integer" }
            }
        })
    }

    #[test]
    fn no_schema_accepts_anything() {
        assert!(validate_config(&json!({"any": true}), None).is_ok());
        assert!(validate_config(&Value::Null, None).is_ok());
    }

    #[test]
    fn valid_config_passes() {
        let config = json!({"model": "gpt", "temperature": 0.7, "retries": 3});
        assert!(validate_config_for("p", &config, Some(&schema())).is_ok());
    }

    #[test]
    fn missing_required_key_is_rejected() {
        let config = json!({"temperature": 0.7});
        let err = validate_config_for("p", &config, Some(&schema())).unwrap_err();
        assert!(
            matches!(err, PluginError::ConfigInvalid { ref reason, .. } if reason.contains("model"))
        );
    }

    #[test]
    fn wrong_type_is_rejected() {
        let config = json!({"model": "gpt", "retries": "many"});
        let err = validate_config_for("p", &config, Some(&schema())).unwrap_err();
        assert!(
            matches!(err, PluginError::ConfigInvalid { ref plugin_id, .. } if plugin_id == "p")
        );
    }

    #[test]
    fn non_object_config_is_rejected_when_schema_expects_object() {
        let err = validate_config_for("p", &json!([1]), Some(&schema())).unwrap_err();
        assert!(matches!(err, PluginError::ConfigInvalid { .. }));
    }

    #[test]
    fn unknown_keys_are_allowed() {
        let config = json!({"model": "gpt", "extra": {"nested": true}});
        assert!(validate_config_for("p", &config, Some(&schema())).is_ok());
    }
}
