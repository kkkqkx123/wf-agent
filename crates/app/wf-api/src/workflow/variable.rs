//! Variable management API: create, query, and evaluate workflow variables
//! through the `VariableStore` and expression evaluator.

use std::collections::HashMap;

use serde::Serialize;

use wf_workflow::variable::{
    convert_variable_type, create_variable_store, evaluate_expression,
    ExpressionError, VariableStore,
};

/// Result of evaluating an expression.
#[derive(Debug, Clone, Serialize)]
pub struct ExpressionResult {
    pub value: serde_json::Value,
    pub display: String,
}

/// Create a new variable store instance.
pub fn create_store() -> VariableStore {
    create_variable_store()
}

/// Set a variable in the store.
pub fn set_variable(store: &VariableStore, name: &str, value: serde_json::Value) {
    store.insert(name.to_string(), value);
}

/// Get a variable value from the store.
pub fn get_variable(store: &VariableStore, name: &str) -> Option<serde_json::Value> {
    store.get(name).map(|r| r.value().clone())
}

/// Check if a variable exists in the store.
pub fn has_variable(store: &VariableStore, name: &str) -> bool {
    store.contains_key(name)
}

/// Remove a variable from the store.
pub fn remove_variable(store: &VariableStore, name: &str) -> Option<serde_json::Value> {
    store.remove(name).map(|(_, v)| v)
}

/// Get all variables as a map.
pub fn list_variables(store: &VariableStore) -> HashMap<String, serde_json::Value> {
    store.iter().map(|r| (r.key().clone(), r.value().clone())).collect()
}

/// Evaluate an expression against the variable store.
pub fn eval_expression(
    store: &VariableStore,
    expr: &str,
) -> Result<ExpressionResult, ExpressionError> {
    let value = evaluate_expression(expr, store)?;
    let display = match &value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    Ok(ExpressionResult { value, display })
}

/// Convert a variable value to a target type string.
pub fn convert_type(
    variable_name: &str,
    value: serde_json::Value,
    target_type: Option<&str>,
) -> Result<serde_json::Value, crate::ApiError> {
    convert_variable_type(variable_name, value, target_type)
        .map_err(crate::ApiError::execution_with_source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_store_crud() {
        let store = create_store();
        assert!(!has_variable(&store, "x"));
        set_variable(&store, "x", serde_json::json!(42));
        assert!(has_variable(&store, "x"));
        assert_eq!(
            get_variable(&store, "x"),
            Some(serde_json::json!(42))
        );
        remove_variable(&store, "x");
        assert!(!has_variable(&store, "x"));
    }

    #[test]
    fn expression_evaluation() {
        let store = create_store();
        set_variable(&store, "count", serde_json::json!(10));
        set_variable(&store, "name", serde_json::json!("hello"));

        let result = eval_expression(&store, "${count} + 5").unwrap();
        assert_eq!(result.value, serde_json::json!(15));

        let result = eval_expression(&store, "${name}").unwrap();
        assert_eq!(result.value, serde_json::json!("hello"));
    }
}
