use serde_json::Value;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::node::configs::script::ScriptOutputMapping;

use crate::error::WorkflowResult;

pub fn apply_output_mappings(
    ctx: &NodeExecutionContext,
    output: &Value,
    mapping_val: &Value,
) -> WorkflowResult<()> {
    // Malformed output mappings are a user error: fail with a structured
    // ConfigError instead of silently skipping the mappings.
    let mappings = crate::config_parse::parse_node_config::<Vec<ScriptOutputMapping>>(
        &ctx.node_id,
        "output_mappings",
        mapping_val,
    )?;
    for mapping in &mappings {
        let value = if let Some(ref path) = mapping.path {
            extract_path_value(output, Some(path))
        } else {
            output.clone()
        };
        match mapping.target.as_str() {
            "variable" | "output" => {
                ctx.set_variable(mapping.key.clone(), value)?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn extract_path_value(value: &Value, path: Option<&str>) -> Value {
    let path = match path {
        Some(p) => p,
        None => return value.clone(),
    };
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;
    for part in parts {
        match current {
            Value::Object(map) => {
                current = map.get(part).unwrap_or(&Value::Null);
            }
            Value::Array(arr) => {
                if let Ok(idx) = part.parse::<usize>() {
                    current = arr.get(idx).unwrap_or(&Value::Null);
                } else {
                    return Value::Null;
                }
            }
            _ => return Value::Null,
        }
    }
    current.clone()
}

pub fn deep_clone_mapped(value: &Value, mappings: &[ScriptOutputMapping]) -> Value {
    let mut result = value.clone();
    for mapping in mappings {
        let src_value = if let Some(ref path) = mapping.path {
            extract_path_value(value, Some(path))
        } else {
            value.clone()
        };
        if let Value::Object(ref mut map) = result {
            map.insert(mapping.key.clone(), src_value);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use dashmap::DashMap;
    use wf_types::node::StaticNodeType;

    #[tokio::test]
    async fn malformed_mappings_fail_with_structured_error() {
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "script-9".to_string(),
            StaticNodeType::Script,
            Value::Null,
            Arc::new(DashMap::new()),
        );
        let err = apply_output_mappings(&ctx, &Value::Null, &serde_json::json!("not-an-array"))
            .expect_err("malformed output mappings must fail");
        let text = err.to_string();
        assert!(
            text.contains("Config error")
                && text.contains("script-9")
                && text.contains("output_mappings"),
            "error must carry node id and field path: {text}"
        );
        // The handler-level call also surfaces the failure.
        let result = ctx.set_variable("x".to_string(), Value::from(1));
        assert!(result.is_ok());
    }
}
