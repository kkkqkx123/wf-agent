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
            "variable '{name}' expects {}, got {}",
            self.type_name(),
            workflow_value_kind(value),
        ))
    }
}

/// Nested property rule for an object-typed variable. One level only;
/// deeper nesting is checked as the top-level kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableProperty {
    pub r#type: VariableValueType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Top-level workflow input shape shared by every entry gate.
/// Construction enforces the object-or-empty contract so gates share one
/// wording instead of each matching on `Value` again.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowInput(Option<serde_json::Map<String, serde_json::Value>>);

impl WorkflowInput {
    pub fn try_from_option(input: Option<&serde_json::Value>) -> Result<Self, String> {
        match input {
            None | Some(serde_json::Value::Null) => Ok(Self(None)),
            Some(serde_json::Value::Object(map)) => Ok(Self(Some(map.clone()))),
            Some(other) => Err(format!(
                "workflow input must be a JSON object or empty, got {}",
                workflow_value_kind(other),
            )),
        }
    }

    pub fn empty() -> Self {
        Self(None)
    }

    pub fn as_option_value(&self) -> Option<serde_json::Value> {
        self.0.clone().map(serde_json::Value::Object)
    }
}

/// Shared top-level shape check used by engine and API gates.
/// Missing or null is no input, an object passes, anything else fails.
/// Per-variable types are checked separately after declarations load.
pub fn validate_workflow_input_shape(input: Option<&serde_json::Value>) -> Result<(), String> {
    WorkflowInput::try_from_option(input).map(|_| ())
}

/// One workflow input problem with a structured field reference.
///
/// The backend always reports the full set collected by
/// `collect_workflow_input_issues` so callers can fix every problem in
/// one round instead of resubmitting per error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowInputIssue {
    pub field: String,
    pub message: String,
}

impl WorkflowInputIssue {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Collect every workflow input problem without stopping at the first.
///
/// A top-level shape violation makes per-key checks meaningless, so it
/// is reported alone; otherwise all declared-key type mismatches, nested
/// property and element mismatches, unknown keys in strict mode, and all
/// missing required variables are reported together. Unknown keys pass by
/// default for forward compatibility. Explicit null counts as missing and
/// is covered by the required check instead of the type check.
pub fn collect_workflow_input_issues(
    input: Option<&serde_json::Value>,
    variables: Option<&[VariableDefinition]>,
) -> Vec<WorkflowInputIssue> {
    collect_workflow_input_issues_with_policy(input, variables, false)
}

/// Policy variant of [`collect_workflow_input_issues`]. `strict` rejects
/// unknown top-level keys; declarations carry one level of nesting
/// (object properties, array element type) either way.
pub fn collect_workflow_input_issues_with_policy(
    input: Option<&serde_json::Value>,
    variables: Option<&[VariableDefinition]>,
    strict: bool,
) -> Vec<WorkflowInputIssue> {
    let mut issues = Vec::new();
    let Some(variables) = variables else {
        return issues;
    };
    if variables.is_empty() {
        return issues;
    }
    let object = match WorkflowInput::try_from_option(input) {
        Ok(parsed) => parsed.0,
        Err(reason) => {
            issues.push(WorkflowInputIssue::new("input", reason));
            return issues;
        }
    };
    if let Some(object) = object.as_ref() {
        for (key, value) in object {
            if value.is_null() {
                continue;
            }
            let Some(declared) = variables.iter().find(|v| v.name == *key) else {
                if strict {
                    issues.push(WorkflowInputIssue::new(
                        key.clone(),
                        format!("workflow input has unknown variable '{key}'"),
                    ));
                }
                continue;
            };
            let Some(expected) = declared.r#type.as_ref() else {
                continue;
            };
            if !expected.matches(value) {
                issues.push(WorkflowInputIssue::new(
                    key.clone(),
                    format!(
                        "variable '{key}' expects {}, got {}",
                        expected.type_name(),
                        workflow_value_kind(value),
                    ),
                ));
                continue;
            }
            issues.extend(nested_workflow_issues(key, value, declared));
        }
    }
    for name in missing_required_variables(input, Some(variables)) {
        issues.push(WorkflowInputIssue::new(
            name.clone(),
            format!("workflow input is missing required variable '{name}'"),
        ));
    }
    issues
}

/// One level of nesting for an already kind-matched value: object
/// properties and array element types. Deeper structure stays at the
/// top-level kind check.
fn nested_workflow_issues(
    key: &str,
    value: &serde_json::Value,
    declared: &VariableDefinition,
) -> Vec<WorkflowInputIssue> {
    let mut issues = Vec::new();
    if let Some(properties) = declared.properties.as_ref() {
        let serde_json::Value::Object(map) = value else {
            return issues;
        };
        for (prop_name, prop_rule) in properties {
            match map.get(prop_name) {
                None | Some(serde_json::Value::Null) => {
                    if prop_rule.required == Some(true) {
                        issues.push(WorkflowInputIssue::new(
                            format!("{key}.{prop_name}"),
                            format!(
                                "variable '{key}.{prop_name}' is missing required property"
                            ),
                        ));
                    }
                }
                Some(prop_value) => {
                    if !prop_rule.r#type.matches(prop_value) {
                        issues.push(WorkflowInputIssue::new(
                            format!("{key}.{prop_name}"),
                            format!(
                                "variable '{key}.{prop_name}' expects {}, got {}",
                                prop_rule.r#type.type_name(),
                                workflow_value_kind(prop_value),
                            ),
                        ));
                    }
                }
            }
        }
    }
    if let Some(element_type) = declared.element_type.as_ref() {
        let serde_json::Value::Array(items) = value else {
            return issues;
        };
        for (index, item) in items.iter().enumerate() {
            if item.is_null() {
                continue;
            }
            if !element_type.matches(item) {
                issues.push(WorkflowInputIssue::new(
                    format!("{key}[{index}]"),
                    format!(
                        "variable '{key}[{index}]' expects {}, got {}",
                        element_type.type_name(),
                        workflow_value_kind(item),
                    ),
                ));
            }
        }
    }
    issues
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
    validate_workflow_input_against_variables_with_policy(input, variables, false)
}

pub fn validate_workflow_input_against_variables_with_policy(
    input: Option<&serde_json::Value>,
    variables: Option<&[VariableDefinition]>,
    strict: bool,
) -> Result<(), String> {
    let issues = collect_workflow_input_issues_with_policy(input, variables, strict);
    if issues.is_empty() {
        return Ok(());
    }
    Err(format!(
        "workflow input validation failed ({} error(s)): {}",
        issues.len(),
        issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>()
            .join("; "),
    ))
}

/// Names of `required` variables absent from the input.
///
/// A missing, null, or explicitly null-valued input counts as an empty
/// object; only declarations with `required == Some(true)` are reported.
/// Explicit null counts as missing so callers can clear a key and still
/// get the default backfill path. Unknown input keys are ignored.
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
        .filter(|v| {
            object.is_none_or(|map| map.get(&v.name).is_none_or(|value| value.is_null()))
        })
        .map(|v| v.name.clone())
        .collect()
}

/// Fill absent input keys from the declared variable defaults.
///
/// Caller non-null values always win; missing keys and explicitly null
/// values are backfilled, and a declared `value` of `Null` means "no
/// default" so nothing is written.
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
        let provided_missing = object.get(&declared.name).is_none_or(|v| v.is_null());
        if !provided_missing {
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
            if value.is_null() {
                continue;
            }
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
    /// One level of object property rules. `None` keeps the top-level kind
    /// check only; deeper nesting is not constrained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, VariableProperty>>,
    /// Expected element kind for array values. Explicit null elements count
    /// as missing and skip the check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_type: Option<VariableValueType>,
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
            properties: None,
            element_type: None,
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
            properties: None,
            element_type: None,
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

    #[test]
    fn all_input_problems_are_reported_together() {
        let variables = vec![
            definition("count", Some(VariableValueType::Number)),
            definition("name", Some(VariableValueType::String)),
            required_definition(
                "city",
                Some(VariableValueType::String),
                serde_json::Value::Null,
            ),
            required_definition(
                "token",
                Some(VariableValueType::String),
                serde_json::Value::Null,
            ),
        ];
        let issues = collect_workflow_input_issues(
            Some(&json!({"count": "many", "name": 1})),
            Some(&variables),
        );
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert_eq!(fields, vec!["count", "name", "city", "token"]);
        let err = validate_workflow_input_against_variables(
            Some(&json!({"count": "many", "name": 1})),
            Some(&variables),
        )
        .expect_err("combined problems must fail");
        assert!(err.contains("4 error(s)"), "{err}");
        for field in ["count", "name", "city", "token"] {
            assert!(err.contains(field), "{err}");
        }
        let shape = collect_workflow_input_issues(Some(&json!([1])), Some(&variables));
        assert_eq!(shape.len(), 1);
        assert_eq!(shape[0].field, "input");
    }

    fn object_variable(
        name: &str,
        properties: HashMap<String, VariableProperty>,
    ) -> VariableDefinition {
        VariableDefinition {
            name: name.to_string(),
            value: serde_json::Value::Null,
            r#type: Some(VariableValueType::Object),
            properties: Some(properties),
            element_type: None,
            scope: None,
            readonly: None,
            required: None,
            metadata: None,
        }
    }

    fn array_variable(name: &str, element: VariableValueType) -> VariableDefinition {
        VariableDefinition {
            name: name.to_string(),
            value: serde_json::Value::Null,
            r#type: Some(VariableValueType::Array),
            properties: None,
            element_type: Some(element),
            scope: None,
            readonly: None,
            required: None,
            metadata: None,
        }
    }

    #[test]
    fn nested_properties_and_elements_are_checked_one_level() {
        let variables = vec![
            object_variable(
                "user",
                HashMap::from([(
                    "age".into(),
                    VariableProperty {
                        r#type: VariableValueType::Number,
                        required: Some(true),
                    },
                )]),
            ),
            array_variable("tags", VariableValueType::String),
        ];
        assert!(collect_workflow_input_issues(
            Some(&json!({"user": {"age": 3}, "tags": ["a"]})),
            Some(&variables),
        )
        .is_empty());
        let issues = collect_workflow_input_issues(
            Some(&json!({"user": {"age": "old"}, "tags": ["a", 1]})),
            Some(&variables),
        );
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"user.age"), "{fields:?}");
        assert!(fields.contains(&"tags[1]"), "{fields:?}");
        let missing = collect_workflow_input_issues(
            Some(&json!({"user": {}, "tags": []})),
            Some(&variables),
        );
        assert!(missing.iter().any(|issue| issue.field == "user.age"));
    }

    #[test]
    fn explicit_null_counts_as_missing_and_backfills() {
        let variables = vec![required_definition(
            "city",
            Some(VariableValueType::String),
            json!("paris"),
        )];
        assert_eq!(
            missing_required_variables(Some(&json!({"city": null})), Some(&variables)),
            vec!["city".to_string()]
        );
        let filled =
            apply_workflow_input_defaults(Some(json!({"city": null})), Some(&variables));
        assert_eq!(filled, json!({"city": "paris"}));
        assert!(collect_workflow_input_issues(
            Some(&json!({"city": null})),
            Some(&variables),
        )
        .iter()
        .all(|issue| issue.field == "city" && issue.message.contains("missing")));
    }

    #[test]
    fn strict_policy_rejects_unknown_top_level_keys() {
        let variables = vec![definition("count", Some(VariableValueType::Number))];
        assert!(collect_workflow_input_issues(
            Some(&json!({"count": 1, "extra": 1})),
            Some(&variables),
        )
        .is_empty());
        let issues = collect_workflow_input_issues_with_policy(
            Some(&json!({"count": 1, "extra": 1})),
            Some(&variables),
            true,
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "extra");
    }
}
