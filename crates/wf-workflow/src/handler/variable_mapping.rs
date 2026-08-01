use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;

use crate::error::{WorkflowError, WorkflowResult};
use crate::variable::VariableResolver;

/// Snapshot copy of every parent variable into the target map.
pub fn inherit_all_variables(
    parent: &Arc<DashMap<String, Value>>,
    target: &Arc<DashMap<String, Value>>,
) {
    for entry in parent.iter() {
        target.insert(entry.key().clone(), entry.value().clone());
    }
}

/// Apply `variable_inputs` mapping: resolve `source_path` from the parent
/// variables and store it under `internal_name` in the child variables.
/// Missing required inputs fail.
pub fn apply_variable_inputs(
    config: &Value,
    parent: &Arc<DashMap<String, Value>>,
    child: &Arc<DashMap<String, Value>>,
) -> WorkflowResult<()> {
    let Some(inputs) = config.get("variable_inputs").and_then(|v| v.as_array()) else {
        return Ok(());
    };

    for entry in inputs {
        let source_path = entry.get("source_path").and_then(|v| v.as_str());
        let internal_name = entry.get("internal_name").and_then(|v| v.as_str());
        let required = entry
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let default_value = entry.get("default_value");

        let (Some(source_path), Some(internal_name)) = (source_path, internal_name) else {
            continue;
        };

        let placeholder = format!("${{{}}}", source_path);
        let resolved = VariableResolver::resolve(&Value::String(placeholder.clone()), parent);
        let unresolved = matches!(&resolved, Value::String(s) if *s == placeholder);

        if unresolved {
            if required && default_value.is_none() {
                return Err(WorkflowError::VariableError(format!(
                    "Required variable input '{}' (mapped to '{}') is missing",
                    source_path, internal_name
                )));
            }
            if let Some(default) = default_value {
                child.insert(internal_name.to_string(), default.clone());
            }
        } else {
            child.insert(internal_name.to_string(), resolved);
        }
    }
    Ok(())
}

/// Apply `variable_outputs` mapping: read `internal_name` from the child
/// variables and write it to `target_path` in the parent variables.
pub fn apply_variable_outputs(
    config: &Value,
    child: &Arc<DashMap<String, Value>>,
    parent: &Arc<DashMap<String, Value>>,
) {
    let Some(outputs) = config.get("variable_outputs").and_then(|v| v.as_array()) else {
        return;
    };

    for entry in outputs {
        let internal_name = entry.get("internal_name").and_then(|v| v.as_str());
        let target_path = entry.get("target_path").and_then(|v| v.as_str());

        let (Some(internal_name), Some(target_path)) = (internal_name, target_path) else {
            continue;
        };

        let Some(value) = child.get(internal_name).map(|v| v.clone()) else {
            continue;
        };

        set_variable_path(parent, target_path, value);
    }
}

/// Write `value` into the variable map at a dotted path
/// (`a.b.c` creates nested objects).
pub fn set_variable_path(vars: &Arc<DashMap<String, Value>>, path: &str, value: Value) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.len() == 1 {
        vars.insert(parts[0].to_string(), value);
        return;
    }

    let mut current = vars
        .get(parts[0])
        .map(|v| v.clone())
        .unwrap_or(Value::Object(serde_json::Map::new()));
    let mut target = &mut current;

    for part in &parts[1..parts.len() - 1] {
        if !target.is_object() {
            *target = Value::Object(serde_json::Map::new());
        }
        let entry = target.as_object_mut().unwrap().entry((*part).to_string());
        let entry_ref = entry.or_insert_with(|| Value::Object(serde_json::Map::new()));
        target = entry_ref;
    }

    if let Value::Object(map) = target {
        map.insert(parts.last().unwrap().to_string(), value);
    }
    vars.insert(parts[0].to_string(), current);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_variable_path() {
        let vars = Arc::new(DashMap::new());
        set_variable_path(&vars, "a.b.c", Value::from(1));
        assert_eq!(
            vars.get("a").unwrap().clone(),
            serde_json::json!({"b": {"c": 1}})
        );
        set_variable_path(&vars, "a.b.d", Value::from(2));
        assert_eq!(
            vars.get("a").unwrap().clone(),
            serde_json::json!({"b": {"c": 1, "d": 2}})
        );
        set_variable_path(&vars, "top", Value::from(3));
        assert_eq!(vars.get("top").unwrap().clone(), Value::from(3));
    }

    #[test]
    fn test_variable_inputs_mapping() {
        let parent = Arc::new(DashMap::new());
        parent.insert("user".to_string(), serde_json::json!({"name": "alice"}));
        let child = Arc::new(DashMap::new());

        let config = serde_json::json!({
            "variable_inputs": [
                {"source_path": "user.name", "internal_name": "name", "required": true},
                {"source_path": "missing", "internal_name": "fallback", "required": false, "default_value": "dflt"}
            ]
        });
        apply_variable_inputs(&config, &parent, &child).unwrap();
        assert_eq!(child.get("name").unwrap().clone(), Value::from("alice"));
        assert_eq!(child.get("fallback").unwrap().clone(), Value::from("dflt"));
    }

    #[test]
    fn test_variable_inputs_required_missing() {
        let parent = Arc::new(DashMap::new());
        let child = Arc::new(DashMap::new());
        let config = serde_json::json!({
            "variable_inputs": [
                {"source_path": "nope", "internal_name": "x", "required": true}
            ]
        });
        assert!(apply_variable_inputs(&config, &parent, &child).is_err());
    }

    #[test]
    fn test_variable_outputs_mapping() {
        let child = Arc::new(DashMap::new());
        child.insert("result".to_string(), Value::from(42));
        let parent = Arc::new(DashMap::new());

        let config = serde_json::json!({
            "variable_outputs": [
                {"internal_name": "result", "target_path": "out.value"}
            ]
        });
        apply_variable_outputs(&config, &child, &parent);
        assert_eq!(
            parent.get("out").unwrap().clone(),
            serde_json::json!({"value": 42})
        );
    }
}
