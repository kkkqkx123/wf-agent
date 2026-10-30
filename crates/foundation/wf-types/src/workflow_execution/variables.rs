use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::Metadata;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VariableValueType {
    Number,
    String,
    Boolean,
    Array,
    Object,
}

impl VariableValueType {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::String => "string",
            Self::Boolean => "boolean",
            Self::Array => "array",
            Self::Object => "object",
        }
    }

    pub fn matches(&self, value: &serde_json::Value) -> bool {
        match self {
            Self::Number => value.is_number(),
            Self::String => value.is_string(),
            Self::Boolean => value.is_boolean(),
            Self::Array => value.is_array(),
            Self::Object => value.is_object(),
        }
    }

    pub fn validate_value(&self, name: &str, value: &serde_json::Value) -> Result<(), String> {
        if self.matches(value) {
            return Ok(());
        }
        Err(format!(
            "variable '{}' expects {}, got {}",
            name,
            self.type_name(),
            workflow_value_kind(value),
        ))
    }
}

fn workflow_value_kind(value: &serde_json::Value) -> &'static str {
    if value.is_null() {
        "null"
    } else if value.is_array() {
        "array"
    } else if value.is_string() {
        "string"
    } else if value.is_number() {
        "number"
    } else if value.is_boolean() {
        "boolean"
    } else {
        "object"
    }
}

pub fn validate_workflow_input_against_variables(
    input: Option<&serde_json::Value>,
    variables: Option<&[VariableDefinition]>,
) -> Result<(), String> {
    let Some(variables) = variables else {
        return Ok(());
    };
    if variables.is_empty() {
        return Ok(());
    }
    let input = match input {
        None | Some(serde_json::Value::Null) => return Ok(()),
        Some(serde_json::Value::Object(_)) => input.expect("object input"),
        Some(other) => {
            return Err(format!(
                "workflow input must be a JSON object or empty, got {}",
                workflow_value_kind(other),
            ));
        }
    };
    let object = input.as_object().expect("object input");
    for (key, value) in object {
        let Some(declared) = variables.iter().find(|v| v.name == *key) else {
            continue;
        };
        let Some(expected) = declared.r#type.as_ref() else {
            continue;
        };
        expected.validate_value(key, value)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableDefinition {
    pub name: String,
    pub value: serde_json::Value,
    pub r#type: Option<VariableValueType>,
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableScope {
    pub variables: HashMap<String, serde_json::Value>,
    pub parent_scope: Option<Box<VariableScope>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn definition(name: &str, r#type: Option<VariableValueType>) -> VariableDefinition {
        VariableDefinition {
            name: name.to_string(),
            value: serde_json::Value::Null,
            r#type,
            scope: None,
            readonly: None,
            metadata: None,
        }
    }

    #[test]
    fn value_type_matches_json_shapes() {
        assert!(VariableValueType::Number.matches(&json!(1)));
        assert!(VariableValueType::String.matches(&json!("x")));
        assert!(VariableValueType::Boolean.matches(&json!(true)));
        assert!(VariableValueType::Array.matches(&json!([1])));
        assert!(VariableValueType::Object.matches(&json!({"a": 1})));
        assert!(!VariableValueType::Number.matches(&json!("1")));
    }

    #[test]
    fn typed_input_keys_are_checked_unknown_keys_pass() {
        let variables = vec![
            definition("count", Some(VariableValueType::Number)),
            definition("name", Some(VariableValueType::String)),
            definition("untyped", None),
        ];
        assert!(validate_workflow_input_against_variables(
            Some(&json!({"count": 1})),
            Some(&variables)
        )
        .is_ok());
        assert!(validate_workflow_input_against_variables(
            Some(&json!({"unknown": "anything"})),
            Some(&variables)
        )
        .is_ok());
        let err = validate_workflow_input_against_variables(
            Some(&json!({"count": "many"})),
            Some(&variables),
        )
        .expect_err("string for number must fail");
        assert!(err.contains("count"), "{err}");
        assert!(err.contains("number"), "{err}");
    }

    #[test]
    fn empty_input_always_passes() {
        let variables = vec![definition("count", Some(VariableValueType::Number))];
        assert!(validate_workflow_input_against_variables(None, Some(&variables)).is_ok());
        assert!(validate_workflow_input_against_variables(
            Some(&serde_json::Value::Null),
            Some(&variables)
        )
        .is_ok());
    }
}
