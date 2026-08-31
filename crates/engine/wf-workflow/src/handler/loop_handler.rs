use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::loop_state::{
    current_item, enter_loop, exit_loop, find_loop, loop_condition_met, update_loop, LoopState,
    LOOP_MAX_ITERATIONS_CAP_KEY, MAX_ITERATIONS_CAP,
};
use crate::variable::VariableResolver;

pub struct LoopStartHandler;

#[async_trait]
impl NodeHandler for LoopStartHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::LoopStart
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl LoopStartHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        let loop_id = config
            .get("loop_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&ctx.node_id)
            .to_string();

        let max_iterations = config
            .get("max_iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as u32;
        // Runtime-injected cap (from limits configuration) wins over the
        // built-in constant when present.
        let max_iterations_cap = ctx
            .variables
            .get(LOOP_MAX_ITERATIONS_CAP_KEY)
            .and_then(|v| v.as_u64())
            .unwrap_or(MAX_ITERATIONS_CAP as u64) as u32;
        if max_iterations > max_iterations_cap {
            return Err(WorkflowError::LoopError(format!(
                "max_iterations ({}) exceeds the allowed limit ({}) for loop '{}'",
                max_iterations, max_iterations_cap, loop_id
            )));
        }

        let on_iteration_failure = config
            .get("on_iteration_failure")
            .and_then(|v| v.as_str())
            .unwrap_or("fail")
            .to_string();
        let max_consecutive_failures = config
            .get("max_consecutive_failures")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let break_condition = config.get("break_condition").and_then(|c| c.as_str());

        // First visit: resolve the iterable, import variable_inputs and push
        // the loop state.
        if find_loop(&ctx.variables, &loop_id).is_none() {
            let iterable = resolve_iterable(config, ctx)?;
            let variable_name = config
                .get("data_source")
                .and_then(|d| d.get("variable_name"))
                .and_then(|v| v.as_str())
                .map(String::from);

            let mut imported_variables = Vec::new();
            if let Some(inputs) = config.get("variable_inputs").and_then(|v| v.as_array()) {
                for input in inputs {
                    if let Some(name) = input.get("internal_name").and_then(|v| v.as_str()) {
                        imported_variables.push(name.to_string());
                    }
                }
                crate::handler::variable_mapping::apply_variable_inputs(
                    config,
                    &ctx.variables,
                    &ctx.variables,
                )?;
            }

            enter_loop(
                &ctx.variables,
                LoopState {
                    loop_id: loop_id.clone(),
                    iterable,
                    current_index: 0,
                    max_iterations,
                    iteration_count: 0,
                    variable_name,
                    consecutive_failures: 0,
                    total_failures: 0,
                    iteration_failed: false,
                    on_iteration_failure,
                    max_consecutive_failures,
                    imported_variables,
                    iteration_started: false,
                    iteration_nodes: Vec::new(),
                },
            );
        }

        let mut state = find_loop(&ctx.variables, &loop_id)
            .ok_or_else(|| WorkflowError::LoopError("loop state missing".to_string()))?;

        // The current iteration has already been started (retry / checkpoint
        // resume re-entry): pass through without advancing.
        if state.iteration_started {
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        }

        // Symmetric break condition on LOOP_START. When the break fires the
        // loop terminates and the flow routes to the loop's LOOP_END (the
        // exit point), which forwards through its outgoing edges.
        if let Some(cond) = break_condition {
            if evaluate_condition(ctx, cond) {
                exit_loop(&ctx.variables, &loop_id);
                let next = find_loop_end_node(ctx, &loop_id).map(|id| vec![id]);
                return Ok(NodeExecutionResult {
                    output: ctx.input.clone(),
                    next_node_ids: next.unwrap_or_default(),
                    metadata: HashMap::new(),
                });
            }
        }

        if !loop_condition_met(&state) {
            exit_loop(&ctx.variables, &loop_id);
            let next = find_loop_end_node(ctx, &loop_id).map(|id| vec![id]);
            return Ok(NodeExecutionResult {
                output: ctx.input.clone(),
                next_node_ids: next.unwrap_or_default(),
                metadata: HashMap::new(),
            });
        }

        let item = current_item(&state).ok_or_else(|| {
            WorkflowError::LoopError(format!(
                "Iterable exhausted at index {} for loop '{}'",
                state.current_index, loop_id
            ))
        })?;
        if let Some(ref var_name) = state.variable_name {
            ctx.set_variable(var_name.clone(), item)?;
        }

        state.iteration_count += 1;
        state.current_index += 1;
        state.iteration_started = true;
        // A new iteration begins: reset the per-iteration completion
        // tracking.
        state.iteration_nodes = Vec::new();
        update_loop(&ctx.variables, state);

        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

pub struct LoopEndHandler;

#[async_trait]
impl NodeHandler for LoopEndHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::LoopEnd
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl LoopEndHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let loop_id = config
            .get("loop_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("default")
            .to_string();

        let break_condition = config.get("break_condition").and_then(|c| c.as_str());
        let loop_start_node_id = config.get("loop_start_node_id").and_then(|t| t.as_str());

        let Some(mut state) = find_loop(&ctx.variables, &loop_id) else {
            // No active loop state: nothing to evaluate, flow forward.
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        };

        // Failure bookkeeping of the finished iteration.
        let iteration_failed = state.iteration_failed;
        if iteration_failed {
            state.consecutive_failures += 1;
            state.total_failures += 1;
        } else {
            state.consecutive_failures = 0;
        }
        state.iteration_failed = false;

        // Iteration failure strategy (evaluated here).
        let mut terminate = false;
        if iteration_failed {
            match state.on_iteration_failure.as_str() {
                "fail" => {
                    exit_loop(&ctx.variables, &loop_id);
                    return Err(WorkflowError::LoopError(format!(
                        "Loop '{}' terminated after a failed iteration (on_iteration_failure=fail); total failures: {}",
                        loop_id, state.total_failures
                    )));
                }
                "skip" => {
                    terminate = true;
                }
                _ => {
                    // continue: keep iterating unless the consecutive
                    // failure threshold is reached.
                    if state.max_consecutive_failures > 0
                        && state.consecutive_failures >= state.max_consecutive_failures
                    {
                        terminate = true;
                    }
                }
            }
        }

        if !terminate {
            if let Some(cond) = break_condition {
                if evaluate_condition(ctx, cond) {
                    terminate = true;
                }
            }
        }
        if !terminate && !loop_condition_met(&state) {
            terminate = true;
        }

        let mut metadata = HashMap::new();
        metadata.insert("loop_id".to_string(), Value::String(loop_id.clone()));
        metadata.insert(
            "iteration".to_string(),
            Value::Number(state.iteration_count.into()),
        );
        metadata.insert(
            "total_failures".to_string(),
            Value::Number(state.total_failures.into()),
        );

        if terminate {
            exit_loop(&ctx.variables, &loop_id);
            metadata.insert("should_continue".to_string(), Value::Bool(false));
            return Ok(NodeExecutionResult {
                output: ctx.input.clone(),
                next_node_ids: Vec::new(),
                metadata,
            });
        }

        // Continue with the next iteration: acknowledge the consumed
        // iteration so the next LOOP_START visit advances the state.
        state.iteration_started = false;
        update_loop(&ctx.variables, state);

        let next_node_ids = if let Some(target) = loop_start_node_id {
            vec![target.to_string()]
        } else {
            Vec::new()
        };
        metadata.insert("should_continue".to_string(), Value::Bool(true));

        Ok(NodeExecutionResult {
            output: ctx.input.clone(),
            next_node_ids,
            metadata,
        })
    }
}

/// Resolve the `data_source.iterable` config: `{{scope.path}}` /
/// `${path}` variable expressions or a direct iterable value (array,
/// object, number, string).
fn resolve_iterable(config: &Value, ctx: &NodeExecutionContext) -> WorkflowResult<Value> {
    let Some(raw) = config.get("data_source").and_then(|d| d.get("iterable")) else {
        // Counting loop: no data source.
        return Ok(Value::Null);
    };

    let resolved = match raw {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
                let path = &trimmed[2..trimmed.len() - 2];
                resolve_expression(path, ctx)?
            } else if trimmed.starts_with("${") && trimmed.ends_with("}") {
                VariableResolver::resolve_str(trimmed, &ctx.variables)
            } else if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                parsed
            } else {
                Value::String(s.clone())
            }
        }
        other => other.clone(),
    };

    if resolved.is_null() {
        return Err(WorkflowError::LoopError(format!(
            "Iterable resolved to null: {}",
            raw
        )));
    }
    if !matches!(
        resolved,
        Value::Array(_) | Value::Object(_) | Value::Number(_) | Value::String(_)
    ) {
        return Err(WorkflowError::LoopError(format!(
            "Iterable must be an array, object, number, string, or variable expression like {{input.list}}. Got: {}",
            resolved
        )));
    }
    Ok(resolved)
}

/// Resolve a `scope.path` expression: `input.*` reads the node input,
/// `execution.*` reads the execution variables, `output.*` is not
/// available in the loop context.
fn resolve_expression(path: &str, ctx: &NodeExecutionContext) -> WorkflowResult<Value> {
    let trimmed = path.trim();
    let Some((scope, rest)) = trimmed.split_once('.') else {
        return Err(WorkflowError::LoopError(format!(
            "Invalid variable scope in iterable expression '{}'. Supported scopes: input, output, execution",
            trimmed
        )));
    };

    let expression = match scope {
        "input" => format!("${{input.{}}}", rest),
        "execution" => format!("${{{}}}", rest),
        "output" => {
            return Err(WorkflowError::LoopError(
                "Iterable expression uses the 'output' scope, which is not available in the loop context"
                    .to_string(),
            ))
        }
        other => {
            return Err(WorkflowError::LoopError(format!(
                "Invalid variable scope '{}'. Supported scopes: input, output, execution",
                other
            )))
        }
    };

    let resolved = VariableResolver::resolve(&Value::String(expression.clone()), &ctx.variables);
    if matches!(&resolved, Value::String(s) if *s == expression) {
        return Err(WorkflowError::LoopError(format!(
            "Variable '{}' not found in the loop context",
            trimmed
        )));
    }
    Ok(resolved)
}

/// Evaluate a break condition string against the execution variables.
/// Evaluation errors are non-fatal (a failing condition does not break);
/// a failing condition does not break the loop.
fn evaluate_condition(ctx: &NodeExecutionContext, condition: &str) -> bool {
    let mut vars = HashMap::new();
    for entry in ctx.variables.iter() {
        vars.insert(entry.key().clone(), entry.value().clone());
    }
    match wf_core::condition::ConditionEvaluator::evaluate(condition, &vars) {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!(
                "break_condition '{}' evaluation failed (treated as false): {}",
                condition,
                e
            );
            false
        }
    }
}

/// Locate the LOOP_END node of `loop_id` in the execution graph (used as the
/// exit routing target when LOOP_START terminates the loop).
fn find_loop_end_node(ctx: &NodeExecutionContext, loop_id: &str) -> Option<String> {
    let graph = ctx.graph_structure.as_ref()?;
    graph.nodes.iter().find_map(|n| {
        if n.node_type != "LOOP_END" {
            return None;
        }
        let id = n.inner.get("loop_id").and_then(|v| v.as_str())?;
        if id == loop_id {
            Some(n.id.clone())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashmap::DashMap;
    use std::sync::Arc;

    fn ctx_with(config: Value) -> NodeExecutionContext {
        let mut ctx = NodeExecutionContext::new(
            wf_common::generate_id(),
            "ls".to_string(),
            StaticNodeType::LoopStart,
            Value::Null,
            Arc::new(DashMap::new()),
        );
        ctx.node_config = Some(config);
        ctx
    }

    #[test]
    fn resolve_iterable_forms() {
        let ctx = ctx_with(serde_json::json!({}));
        // Counting loop
        assert_eq!(
            resolve_iterable(&serde_json::json!({}), &ctx).unwrap(),
            Value::Null
        );
        // Raw array (config carries the value directly)
        assert_eq!(
            resolve_iterable(
                &serde_json::json!({"data_source": {"iterable": ["a", "b"], "variable_name": "x"}}),
                &ctx
            )
            .unwrap(),
            serde_json::json!(["a", "b"])
        );
        // Raw number
        assert_eq!(
            resolve_iterable(
                &serde_json::json!({"data_source": {"iterable": 5, "variable_name": "x"}}),
                &ctx
            )
            .unwrap(),
            serde_json::json!(5)
        );
        // String literal
        assert_eq!(
            resolve_iterable(
                &serde_json::json!({"data_source": {"iterable": "abc", "variable_name": "x"}}),
                &ctx
            )
            .unwrap(),
            Value::String("abc".to_string())
        );
    }

    #[test]
    fn resolve_iterable_from_variables() {
        let ctx = ctx_with(serde_json::json!({}));
        ctx.variables
            .insert("items".to_string(), serde_json::json!([1, 2, 3]));
        let cfg = serde_json::json!({"data_source": {"iterable": "{{execution.items}}", "variable_name": "x"}});
        assert_eq!(
            resolve_iterable(&cfg, &ctx).unwrap(),
            serde_json::json!([1, 2, 3])
        );

        let cfg =
            serde_json::json!({"data_source": {"iterable": "${items}", "variable_name": "x"}});
        assert_eq!(
            resolve_iterable(&cfg, &ctx).unwrap(),
            serde_json::json!([1, 2, 3])
        );

        let cfg = serde_json::json!({"data_source": {"iterable": "{{execution.missing}}", "variable_name": "x"}});
        let err = resolve_iterable(&cfg, &ctx).unwrap_err();
        assert!(err.to_string().contains("not found"), "{}", err);

        let cfg = serde_json::json!({"data_source": {"iterable": "{{bogus.items}}", "variable_name": "x"}});
        let err = resolve_iterable(&cfg, &ctx).unwrap_err();
        assert!(
            err.to_string().contains("Invalid variable scope"),
            "{}",
            err
        );
    }

    #[tokio::test]
    async fn loop_cap_injection_controls_max_iterations() {
        // A runtime-injected cap (from limits configuration) wins over the
        // built-in constant: a loop declaring more iterations is rejected.
        let mut ctx = ctx_with(serde_json::json!({
            "loop_id": "loop-1",
            "max_iterations": 5000,
        }));
        ctx.variables.insert(
            LOOP_MAX_ITERATIONS_CAP_KEY.to_string(),
            serde_json::json!(1000),
        );
        let handler = LoopStartHandler;
        let err = handler
            .execute_inner(&mut ctx)
            .await
            .err()
            .expect("expected LoopError from injected cap");
        assert!(
            err.to_string().contains("exceeds the allowed limit"),
            "unexpected error: {err}"
        );

        // Without injection the built-in cap (10000) applies.
        let mut ctx = ctx_with(serde_json::json!({
            "loop_id": "loop-2",
            "max_iterations": 10001,
        }));
        let err = handler
            .execute_inner(&mut ctx)
            .await
            .err()
            .expect("expected LoopError from built-in cap");
        assert!(
            err.to_string().contains("exceeds the allowed limit"),
            "unexpected error: {err}"
        );
    }
}
