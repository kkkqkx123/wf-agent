
use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

pub struct ContextProcessorHandler;

#[async_trait]
impl NodeHandler for ContextProcessorHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::ContextProcessor
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        if let Some(set_fields) = config.get("set").and_then(|v| v.as_object()) {
            for (key, value) in set_fields {
                let resolved = crate::variable::VariableResolver::resolve(value, &ctx.variables);
                ctx.set_variable(key.clone(), resolved);
            }
        }

        if let Some(remove_fields) = config.get("remove").and_then(|v| v.as_array()) {
            for key in remove_fields {
                if let Some(k) = key.as_str() {
                    ctx.variables.remove(k);
                }
            }
        }

        if let Some(transform_fields) = config.get("transform").and_then(|v| v.as_object()) {
            for (key, transform) in transform_fields {
                if let Some(op) = transform.get("operation").and_then(|v| v.as_str()) {
                    let current = ctx.get_variable(key).unwrap_or(Value::Null);
                    let result = match op {
                        "uppercase" => {
                            if let Value::String(s) = current {
                                Value::String(s.to_uppercase())
                            } else {
                                current
                            }
                        }
                        "lowercase" => {
                            if let Value::String(s) = current {
                                Value::String(s.to_lowercase())
                            } else {
                                current
                            }
                        }
                        "increment" => {
                            if let Value::Number(n) = current {
                                let v = n.as_f64().unwrap_or(0.0) + 1.0;
                                Value::Number(serde_json::Number::from_f64(v).unwrap_or(n.clone()))
                            } else {
                                current
                            }
                        }
                        "decrement" => {
                            if let Value::Number(n) = current {
                                let v = n.as_f64().unwrap_or(0.0) - 1.0;
                                Value::Number(serde_json::Number::from_f64(v).unwrap_or(n.clone()))
                            } else {
                                current
                            }
                        }
                        "toString" => {
                            Value::String(current.to_string())
                        }
                        _ => current,
                    };
                    ctx.set_variable(key.clone(), result);
                }
            }
        }

        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}
