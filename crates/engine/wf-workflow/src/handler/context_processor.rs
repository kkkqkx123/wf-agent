use async_trait::async_trait;
use serde_json::{json, Value};
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::{execute_fold, FoldParams};
use wf_integration::FoldPolicy;
use wf_types::node::configs::variable_operation::VariableOperationConfig;
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

/// Outcome serialisation key of the fold stage report.
const KEY_SKIPPED: &str = "skipped";

fn usize_config(config: &Value, key: &str, fallback: usize) -> WorkflowResult<usize> {
    match config.get(key) {
        None | Some(Value::Null) => Ok(fallback),
        Some(Value::Number(n)) => n.as_u64().map(|v| v as usize).ok_or_else(|| {
            WorkflowError::OperationError(format!(
                "context processor node config '{key}' must be a number"
            ))
        }),
        Some(_) => Err(WorkflowError::OperationError(format!(
            "context processor node config '{key}' must be a number"
        ))),
    }
}

fn service_config(config: &Value, key: &str) -> WorkflowResult<Option<String>> {
    match config.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(WorkflowError::OperationError(format!(
            "context processor node config '{key}' must be a string"
        ))),
    }
}

fn skip_report(reason: impl Into<String>, folded_count: usize) -> Value {
    json!({
        "folded_count": folded_count,
        KEY_SKIPPED: reason.into(),
    })
}

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

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl ContextProcessorHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().cloned().unwrap_or(Value::Null);

        // Service-backed fold runs first when selected: read the source
        // array, fold oversized file contents through the external
        // service, and write the result back through the register path.
        // Every service failure maps to a skip report; folding never fails
        // the workflow on service grounds.
        if config
            .get("fold")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return self.execute_fold(ctx, &config).await;
        }

        // Message-context operations run first when configured: read the
        // source array, apply the stateless operation, and write the result
        // back through the register path (ledger version advances, stale
        // compression results are invalidated like any other replacement).
        if let Some(operation_value) = config.get("operation_config") {
            let operation: wf_types::message::MessageOperationConfig =
                serde_json::from_value(operation_value.clone()).map_err(|e| {
                    WorkflowError::OperationError(format!(
                        "Invalid operation_config for message context: {e}"
                    ))
                })?;
            let (source, target) = crate::message_context::resolve_source_target(
                &config,
                crate::message_context::DEFAULT_CONTEXT_ID,
            );
            let messages = crate::message_context::get_context(&ctx.variables, &source);
            let (result, stats) = wf_execution_shared::message_ops::apply(&messages, &operation);
            crate::message_context::register_context(&ctx.variables, &target, result);
            let mut output = ctx.input.clone();
            if let Value::Object(map) = &mut output {
                map.insert(
                    "message_count".to_string(),
                    Value::Number(serde_json::Number::from(stats.total_after as u64)),
                );
            }
            return Ok(NodeExecutionResult::simple(output));
        }

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
                ctx.set_variable(target_variable, result)?;
            }
            VariableOperationConfig::Transform {
                source_variable,
                target_variable,
                transform,
            } => {
                let current = get_variable(ctx, &source_variable).unwrap_or(Value::Null);
                let result = transform_value(&current, &transform);
                ctx.set_variable(target_variable, result)?;
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
                ctx.set_variable(target, base)?;
            }
        }

        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }

    async fn execute_fold(
        &self,
        ctx: &mut NodeExecutionContext,
        config: &Value,
    ) -> WorkflowResult<NodeExecutionResult> {
        let defaults = FoldPolicy::default();
        let (source, target) = crate::message_context::resolve_source_target(
            config,
            crate::message_context::DEFAULT_CONTEXT_ID,
        );
        let params = FoldParams {
            base_url: service_config(config, "service_base_url")?,
            timeout_ms: usize_config(config, "service_timeout_ms", 60_000)? as u64,
            min_tokens: usize_config(config, "min_tokens", defaults.min_tokens)?,
            max_tokens: usize_config(config, "max_tokens", defaults.max_tokens)?.max(1),
            max_items: usize_config(config, "max_items", defaults.max_items)?.max(1),
            max_batches: usize_config(config, "max_batches", defaults.max_batches as usize)?.max(1)
                as u32,
        };
        let messages = crate::message_context::get_context(&ctx.variables, &source);
        if messages.is_empty() {
            return Ok(NodeExecutionResult::simple(skip_report(
                "empty source context",
                0,
            )));
        }
        let outcome = execute_fold(&messages, &params).await;
        crate::message_context::register_context(&ctx.variables, &target, outcome.messages);
        let written = crate::message_context::get_context(&ctx.variables, &target);
        let folded_tokens = wf_llm::estimate_messages(&written) as usize;
        let mut report = json!({
            "folded_count": outcome.folded_count,
            "original_tokens": outcome.original_tokens,
            "folded_tokens": folded_tokens,
            "notice_headed": outcome.notice_headed,
        });
        if let Some(reason) = outcome.skipped {
            report[KEY_SKIPPED] = Value::String(reason);
        }
        Ok(NodeExecutionResult::simple(report))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use dashmap::DashMap;
    use wf_types::Id;

    fn empty_ctx() -> NodeExecutionContext {
        NodeExecutionContext::new(
            Id::new(),
            "processor-1".into(),
            StaticNodeType::ContextProcessor,
            Value::Null,
            Arc::new(DashMap::new()),
        )
    }

    #[tokio::test]
    async fn processor_fold_skips_empty_source() {
        let handler = ContextProcessorHandler;
        let mut ctx = empty_ctx();
        ctx.node_config = Some(serde_json::json!({ "fold": true }));
        crate::message_context::register_context(&ctx.variables, "current", Vec::new());
        let result = handler.execute(&mut ctx).await.expect("node resolves");
        assert_eq!(
            result.output.get(KEY_SKIPPED).and_then(|v| v.as_str()),
            Some("empty source context")
        );
    }

    #[tokio::test]
    async fn processor_fold_skips_unreachable_service() {
        let handler = ContextProcessorHandler;
        let mut ctx = empty_ctx();
        ctx.node_config = Some(serde_json::json!({
            "fold": true,
            "min_tokens": 1,
            "service_base_url": "http://127.0.0.1:1",
            "service_timeout_ms": 200,
        }));
        let messages = vec![wf_types::message::Message::tool_result(
            "call-1".into(),
            Some("read_file".into()),
            "fn main() {}\n".repeat(600),
            false,
        )];
        crate::message_context::register_context(&ctx.variables, "current", messages);
        let result = handler.execute(&mut ctx).await.expect("node resolves");
        assert!(result.output.get(KEY_SKIPPED).is_some());
    }
}
