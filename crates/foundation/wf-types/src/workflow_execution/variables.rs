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
    let object = match input {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Object(map)) => Some(map),
        Some(other) => {
            return Err(format!(
                "workflow input must be a JSON object or empty, got {}",
                workflow_value_kind(other),
            ));
        }
    };
    if let Some(object) = object {
        for (key, value) in object {
            let Some(declared) = variables.iter().find(|v| v.name == *key) else {
                continue;
            };
            let Some(expected) = declared.r#type.as_ref() else {
                continue;
            };
            expected.validate_value(key, value)?;
        }
    }
    let missing = missing_required_variables(input, Some(variables));
    if !missing.is_empty() {
        return Err(format!(
            "workflow input is missing required variable(s): {}",
            missing.join(", ")
        ));
    }
    Ok(())
}

/// Names of `required` variables absent from the input.
///
/// A missing or null input counts as an empty object; only declarations
/// with `required == Some(true)` are reported. Unknown input keys are
/// ignored.
pub fn missing_required_variables(
    input: Option<&serde_json::Value>,
    variables: Option<&[VariableDefinition]>,
) -> Vec<String> {
    let Some(variables) = variables else {
        return Vec::new();
    };
    let required: Vec<&VariableDefinition> = variables
        .iter()
        .filter(|v| v.required == Some(true))
        .collect();
    if required.is_empty() {
        return Vec::new();
    }
    let object = match input {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Object(map)) => Some(map),
        Some(_) => return Vec::new(),
    };
    required
        .into_iter()
        .filter(|v| object.is_none_or(|map| !map.contains_key(&v.name)))
        .map(|v| v.name.clone())
        .collect()
}

/// Fill absent input keys from the declared variable defaults.
///
/// Caller values always win; only missing keys are backfilled, and a
/// declared `value` of `Null` means "no default" so nothing is written.
/// Non-object inputs pass through untouched.
pub fn apply_workflow_input_defaults(
    input: Option<serde_json::Value>,
    variables: Option<&[VariableDefinition]>,
) -> serde_json::Value {
    let Some(variables) = variables else {
        return input.unwrap_or(serde_json::Value::Null);
    };
    let mut object = match input {
        None | Some(serde_json::Value::Null) => serde_json::Map::new(),
        Some(serde_json::Value::Object(map)) => map,
        Some(other) => return other,
    };
    for declared in variables {
        if object.contains_key(&declared.name) {
            continue;
        }
        if declared.value.is_null() {
            continue;
        }
        object.insert(declared.name.clone(), declared.value.clone());
    }
    serde_json::Value::Object(object)
}

/// Lenient re-check for resumed executions against a possibly updated
/// definition.
///
/// Missing required variables with no backfillable default are hard errors
/// (resume cannot proceed); type mismatches on provided keys are warnings
/// (the persisted input passed the older contract, so recovery continues
/// and the mismatch stays observable instead of blocking the resume).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeInputReport {
    pub hard_errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn validate_resumed_workflow_input(
    input: Option<&serde_json::Value>,
    variables: Option<&[VariableDefinition]>,
) -> ResumeInputReport {
    let Some(variables) = variables else {
        return ResumeInputReport {
            hard_errors: Vec::new(),
            warnings: Vec::new(),
        };
    };
    if variables.is_empty() {
        return ResumeInputReport {
            hard_errors: Vec::new(),
            warnings: Vec::new(),
        };
    }
    let mut report = ResumeInputReport {
        hard_errors: Vec::new(),
        warnings: Vec::new(),
    };
    for name in missing_required_variables(input, Some(variables)) {
        let backfillable = variables
            .iter()
            .any(|v| v.name == name && !v.value.is_null());
        if backfillable {
            continue;
        }
        report.hard_errors.push(format!(
            "resumed input is missing required variable '{name}' with no default; restore from checkpoint or rerun instead"
        ));
    }
    let object = match input {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Object(map)) => Some(map),
        Some(_) => None,
    };
    if let Some(object) = object {
        for (key, value) in object {
            let Some(declared) = variables.iter().find(|v| v.name == *key) else {
                continue;
            };
            let Some(expected) = declared.r#type.as_ref() else {
                continue;
            };
            if let Err(reason) = expected.validate_value(key, value) {
                report.warnings.push(format!("resumed input {reason}"));
            }
        }
    }
    report
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableDefinition {
    pub name: String,
    pub value: serde_json::Value,
    pub r#type: Option<VariableValueType>,
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    /// Whether the execution input must carry this variable. `None`
    /// (including all persisted definitions) means optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
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
            required: None,
            metadata: None,
        }
    }

    fn required_definition(
        name: &str,
        r#type: Option<VariableValueType>,
        default: serde_json::Value,
    ) -> VariableDefinition {
        VariableDefinition {
            name: name.to_string(),
            value: default,
            r#type,
            scope: None,
            readonly: None,
            required: Some(true),
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
    fn required_variables_are_reported_when_missing() {
        let variables = vec![required_definition(
            "city",
            Some(VariableValueType::String),
            serde_json::Value::Null,
        )];
        assert_eq!(
            missing_required_variables(Some(&json!({})), Some(&variables)),
            vec!["city".to_string()]
        );
        assert_eq!(
            missing_required_variables(None, Some(&variables)),
            vec!["city".to_string()]
        );
        assert!(
            missing_required_variables(Some(&json!({"city": "paris"})), Some(&variables))
                .is_empty()
        );
        let err = validate_workflow_input_against_variables(None, Some(&variables))
            .expect_err("missing required must fail");
        assert!(err.contains("city"), "{err}");
    }

    #[test]
    fn defaults_backfill_missing_keys_without_overriding_caller() {
        let variables = vec![required_definition(
            "city",
            Some(VariableValueType::String),
            json!("paris"),
        )];
        let filled = apply_workflow_input_defaults(None, Some(&variables));
        assert_eq!(filled, json!({"city": "paris"}));
        let filled = apply_workflow_input_defaults(Some(json!({"other": 1})), Some(&variables));
        assert_eq!(filled, json!({"other": 1, "city": "paris"}));
        let kept = apply_workflow_input_defaults(Some(json!({"city": "london"})), Some(&variables));
        assert_eq!(kept, json!({"city": "london"}));
        let null_default = vec![required_definition(
            "city",
            Some(VariableValueType::String),
            serde_json::Value::Null,
        )];
        let unfilled = apply_workflow_input_defaults(None, Some(&null_default));
        assert_eq!(unfilled, json!({}));
    }

    #[test]
    fn resumed_input_reports_hard_errors_and_warnings() {
        let variables = vec![
            required_definition("city", Some(VariableValueType::String), json!("paris")),
            required_definition(
                "token",
                Some(VariableValueType::String),
                serde_json::Value::Null,
            ),
            definition("count", Some(VariableValueType::Number)),
        ];
        let report = validate_resumed_workflow_input(None, Some(&variables));
        assert_eq!(report.hard_errors.len(), 1);
        assert!(report.hard_errors[0].contains("token"), "{:?}", report);
        assert!(report.warnings.is_empty());
        let report = validate_resumed_workflow_input(
            Some(&json!({"city": "paris", "count": "many"})),
            Some(&variables),
        );
        assert!(report.hard_errors.iter().any(|e| e.contains("token")));
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("count"), "{:?}", report);
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
