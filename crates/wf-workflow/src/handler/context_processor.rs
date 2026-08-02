use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::configs::variable_operation::VariableOperationConfig;
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

fn get_variable(ctx: &NodeExecutionContext, name: &str) -> Option<Value> {
    ctx.get_variable(name)
}

fn transform_value(current: &Value, op: &str) -> Value {
    match op {
        "uppercase" => {
            if let Value::String(s) = current {
                Value::String(s.to_uppercase())
            } else {
                current.clone()
            }
        }
        "lowercase" => {
            if let Value::String(s) = current {
                Value::String(s.to_lowercase())
            } else {
                current.clone()
            }
        }
        "increment" => {
            if let Value::Number(n) = current {
                let v = n.as_f64().unwrap_or(0.0) + 1.0;
                Value::Number(serde_json::Number::from_f64(v).unwrap_or(n.clone()))
            } else {
                current.clone()
            }
        }
        "decrement" => {
            if let Value::Number(n) = current {
                let v = n.as_f64().unwrap_or(0.0) - 1.0;
                Value::Number(serde_json::Number::from_f64(v).unwrap_or(n.clone()))
            } else {
                current.clone()
            }
        }
        "toString" => Value::String(current.to_string()),
        _ => current.clone(),
    }
}

/// Merge objects field-by-field (arrays concatenated, scalars last-wins).
fn merge_objects(items: &[Value]) -> Value {
    let mut merged: serde_json::Map<String, Value> = serde_json::Map::new();
    for item in items {
        if let Value::Object(map) = item {
            for (key, value) in map {
                match (merged.get_mut(key), value) {
                    (Some(Value::Array(prev)), Value::Array(items)) => {
                        prev.extend(items.clone());
                    }
                    (Some(prev), value) => {
                        *prev = value.clone();
                    }
                    (None, value) => {
                        merged.insert(key.clone(), value.clone());
                    }
                }
            }
        }
    }
    Value::Object(merged)
}

fn aggregate(items: &[Value], mode: &str) -> Value {
    match mode {
        "array" => Value::Array(items.to_vec()),
        "object" => {
            let mut merged: serde_json::Map<String, Value> = serde_json::Map::new();
            for item in items {
                if let Value::Object(map) = item {
                    for (key, value) in map {
                        merged.insert(key.clone(), value.clone());
                    }
                }
            }
            Value::Object(merged)
        }
        _ => merge_objects(items),
    }
}

pub struct ContextProcessorHandler;

#[async_trait]
impl NodeHandler for ContextProcessorHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::ContextProcessor
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().cloned().unwrap_or(Value::Null);

        let Some(operation_value) = config.get("variable_operation") else {
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        };
        let operation: VariableOperationConfig = serde_json::from_value(operation_value.clone())
            .map_err(|e| {
                WorkflowError::OperationError(format!("Invalid variable_operation config: {}", e))
            })?;

        match operation {
            VariableOperationConfig::Aggregate {
                source_variable,
                target_variable,
                aggregate_mode,
            } => {
                let items = get_variable(ctx, &source_variable)
                    .and_then(|v| match v {
                        Value::Array(arr) => Some(arr),
                        Value::Object(map) => Some(vec![Value::Object(map)]),
                        _ => None,
                    })
                    .unwrap_or_default();
                let mode = format!("{:?}", aggregate_mode).to_lowercase();
                let result = aggregate(&items, &mode);
                ctx.set_variable(target_variable, result);
            }
            VariableOperationConfig::Transform {
                source_variable,
                target_variable,
                transform,
            } => {
                let current = get_variable(ctx, &source_variable).unwrap_or(Value::Null);
                let result = transform_value(&current, &transform);
                ctx.set_variable(target_variable, result);
            }
            VariableOperationConfig::BatchUpdate {
                source_variable,
                target_variable,
                updates,
            } => {
                let mut base = match (source_variable.as_deref(), target_variable.as_deref()) {
                    (Some(source), Some(target)) if source == target => {
                        get_variable(ctx, source).unwrap_or(Value::Object(Default::default()))
                    }
                    (Some(source), _) => {
                        get_variable(ctx, source).unwrap_or(Value::Object(Default::default()))
                    }
                    (None, _) => Value::Object(Default::default()),
                };
                if !base.is_object() {
                    base = Value::Object(Default::default());
                }
                if let Value::Object(map) = &mut base {
                    for update in updates {
                        map.insert(update.key, update.value);
                    }
                }
                let target = target_variable
                    .or(source_variable)
                    .unwrap_or_else(|| "variables".to_string());
                ctx.set_variable(target, base);
            }
        }

        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}
