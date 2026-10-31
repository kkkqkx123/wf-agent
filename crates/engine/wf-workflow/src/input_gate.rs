use serde_json::Value;

use wf_types::workflow_execution::WorkflowExecutionOptions;

use crate::error::{WorkflowError, WorkflowResult};

pub fn validate_input_shape(input: Option<&Value>) -> WorkflowResult<()> {
    wf_types::workflow_execution::validate_workflow_input_shape(input)
        .map_err(WorkflowError::VariableError)
}

/// Collect every non-positive runtime budget without stopping at the
/// first so callers can fix the whole entry in one round.
pub fn collect_budget_issues(options: &WorkflowExecutionOptions) -> Vec<String> {
    let positive: [(&str, Option<u64>); 7] = [
        ("max_steps", options.max_steps.map(|v| v as u64)),
        ("timeout", options.timeout),
        ("max_execution_time", options.max_execution_time),
        ("node_timeout", options.node_timeout),
        ("max_pause_duration", options.max_pause_duration),
        (
            "max_navigation_multiplier",
            options.max_navigation_multiplier.map(|v| v as u64),
        ),
        (
            "loop_max_iterations_cap",
            options.loop_max_iterations_cap.map(|v| v as u64),
        ),
    ];
    positive
        .into_iter()
        .filter(|(_, value)| *value == Some(0))
        .map(|(name, _)| format!("workflow {name} must be a positive number when set"))
        .collect()
}

pub fn validate_runtime_budgets(options: &WorkflowExecutionOptions) -> WorkflowResult<()> {
    let issues = collect_budget_issues(options);
    if issues.is_empty() {
        return Ok(());
    }
    Err(WorkflowError::VariableError(format!(
        "workflow budgets validation failed ({} error(s)): {}",
        issues.len(),
        issues.join("; "),
    )))
}

pub fn gate_workflow_request(
    options: &WorkflowExecutionOptions,
    input: Option<&Value>,
) -> WorkflowResult<()> {
    let effective = options.input.as_ref().or(input);
    let mut issues = Vec::new();
    if let Err(WorkflowError::VariableError(reason)) = validate_input_shape(effective) {
        issues.push(reason);
    }
    issues.extend(collect_budget_issues(options));
    if issues.is_empty() {
        return Ok(());
    }
    Err(WorkflowError::VariableError(format!(
        "workflow request validation failed ({} error(s)): {}",
        issues.len(),
        issues.join("; "),
    )))
}

/// Merge the two input channels, backfill declared defaults and enforce
/// the declared type and required contract. Options input wins, top-level
/// input is the fallback. `None` declarations keep the merged value as is.
pub fn resolve_typed_input(
    options_input: Option<Value>,
    fallback_input: Option<Value>,
    variables: Option<&[wf_types::workflow_execution::VariableDefinition]>,
) -> WorkflowResult<Option<Value>> {
    let merged = options_input.or(fallback_input);
    let Some(declarations) = variables else {
        return Ok(merged);
    };
    if declarations.is_empty() {
        return Ok(merged);
    }
    let filled =
        wf_types::workflow_execution::apply_workflow_input_defaults(merged, Some(declarations));
    let filled_option = match filled {
        Value::Null => None,
        other => Some(other),
    };
    wf_types::workflow_execution::validate_workflow_input_against_variables(
        filled_option.as_ref(),
        Some(declarations),
    )
    .map_err(WorkflowError::VariableError)?;
    Ok(filled_option)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn empty_options() -> WorkflowExecutionOptions {
        WorkflowExecutionOptions {
            input: None,
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: None,
            node_timeout: None,
            max_pause_duration: None,
            max_navigation_multiplier: None,
            loop_max_iterations_cap: None,
            file_scope: None,
        }
    }

    #[test]
    fn input_shape_accepts_empty_and_object() {
        assert!(validate_input_shape(None).is_ok());
        assert!(validate_input_shape(Some(&Value::Null)).is_ok());
        assert!(validate_input_shape(Some(&json!({}))).is_ok());
        assert!(validate_input_shape(Some(&json!({"a": 1}))).is_ok());
    }

    #[test]
    fn input_shape_rejects_non_objects() {
        for value in [json!([]), json!("text"), json!(42), json!(true)] {
            assert!(
                validate_input_shape(Some(&value)).is_err(),
                "value {value} must be rejected"
            );
        }
    }

    #[test]
    fn budgets_reject_zero() {
        let mut options = empty_options();
        options.timeout = Some(0);
        assert!(validate_runtime_budgets(&options).is_err());
        options.timeout = Some(100);
        options.max_steps = Some(0);
        assert!(validate_runtime_budgets(&options).is_err());
    }

    #[test]
    fn budgets_report_every_zero_together() {
        let mut options = empty_options();
        options.timeout = Some(0);
        options.max_steps = Some(0);
        options.node_timeout = Some(0);
        let err = validate_runtime_budgets(&options).expect_err("zeros must fail");
        let text = err.to_string();
        assert!(text.contains("3 error(s)"), "{text}");
        for name in ["timeout", "max_steps", "node_timeout"] {
            assert!(text.contains(name), "{text}");
        }
        let err = gate_workflow_request(&options, Some(&json!([1])))
            .expect_err("shape plus budgets must fail together");
        let text = err.to_string();
        assert!(text.contains("must be a JSON object"), "{text}");
        assert!(text.contains("max_steps"), "{text}");
    }

    #[test]
    fn budgets_accept_positive_and_unset() {
        let mut options = empty_options();
        assert!(validate_runtime_budgets(&options).is_ok());
        options.timeout = Some(100);
        options.max_steps = Some(5);
        assert!(validate_runtime_budgets(&options).is_ok());
    }

    #[test]
    fn request_gate_prefers_options_input() {
        let mut options = empty_options();
        options.input = Some(json!({"a": 1}));
        assert!(gate_workflow_request(&options, Some(&json!([]))).is_ok());
        let options = empty_options();
        assert!(gate_workflow_request(&options, Some(&json!([]))).is_err());
    }

    fn typed_declaration(
        name: &str,
        r#type: Option<wf_types::workflow_execution::VariableValueType>,
        required: Option<bool>,
        default: Value,
    ) -> wf_types::workflow_execution::VariableDefinition {
        wf_types::workflow_execution::VariableDefinition {
            name: name.to_string(),
            value: default,
            r#type,
            properties: None,
            element_type: None,
            scope: None,
            readonly: None,
            required,
            metadata: None,
        }
    }

    #[test]
    fn typed_input_backfills_defaults_without_overriding_caller() {
        use wf_types::workflow_execution::VariableValueType;
        let declarations = vec![typed_declaration(
            "city",
            Some(VariableValueType::String),
            None,
            json!("paris"),
        )];
        let resolved =
            resolve_typed_input(None, None, Some(&declarations)).expect("defaults apply");
        assert_eq!(resolved, Some(json!({"city": "paris"})));
        let resolved =
            resolve_typed_input(Some(json!({"city": "london"})), None, Some(&declarations))
                .expect("caller wins");
        assert_eq!(resolved, Some(json!({"city": "london"})));
    }

    #[test]
    fn typed_input_rejects_mismatch_and_missing_required() {
        use wf_types::workflow_execution::VariableValueType;
        let declarations = vec![
            typed_declaration("count", Some(VariableValueType::Number), None, Value::Null),
            typed_declaration(
                "city",
                Some(VariableValueType::String),
                Some(true),
                Value::Null,
            ),
        ];
        assert!(resolve_typed_input(
            Some(json!({"count": "many", "city": "paris"})),
            None,
            Some(&declarations)
        )
        .is_err());
        assert!(resolve_typed_input(Some(json!({"count": 1})), None, Some(&declarations)).is_err());
        assert!(resolve_typed_input(
            Some(json!({"count": 1, "city": "paris", "extra": 1})),
            None,
            Some(&declarations)
        )
        .is_ok());
    }

    #[test]
    fn typed_input_without_declarations_passes_through() {
        let input = Some(json!({"anything": 1}));
        assert_eq!(
            resolve_typed_input(input.clone(), None, None).expect("passthrough"),
            input
        );
        assert_eq!(
            resolve_typed_input(input.clone(), None, Some(&[])).expect("passthrough"),
            input
        );
    }
}
