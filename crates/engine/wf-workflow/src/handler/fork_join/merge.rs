use serde_json::Value;
use wf_execution_shared::context::NodeExecutionContext;

use crate::barrier::BranchResult;
use crate::error::WorkflowResult;

/// Merge strategy: object fields merged field-by-field (arrays concatenated,
/// scalars last-wins). Non-object outputs fall back to last-wins.
pub fn merge_outputs(outputs: &[Value]) -> Value {
    let mut merged: serde_json::Map<String, Value> = serde_json::Map::new();
    for output in outputs {
        match output {
            Value::Object(map) => {
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
            value => {
                return value.clone();
            }
        }
    }
    Value::Object(merged)
}

/// Collect raw `BranchResult` records from a fork output shape
/// (`{ results: [...] }` or legacy `{ outputs: [...] }`) or a plain array.
/// Returns the parsed records plus one detail string per unparseable entry
/// so the caller can surface the loss (the merge never silently drops a
/// branch's result without reporting it).
pub fn collect_branch_records(input: &Value) -> (Vec<BranchResult>, Vec<String>) {
    if let Some(results) = input.get("results").and_then(|v| v.as_array()) {
        return parse_branch_records(results, "results");
    }
    // Legacy `outputs` shape: `[{ branch_id, output, success }]`.
    if let Some(outputs) = input.get("outputs").and_then(|v| v.as_array()) {
        let records = outputs
            .iter()
            .map(|entry| {
                let success = entry
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if success {
                    BranchResult::success(
                        entry
                            .get("branch_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("branch"),
                        entry.get("output").cloned().unwrap_or(Value::Null),
                    )
                } else {
                    BranchResult::failure(
                        entry
                            .get("branch_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("branch"),
                        entry
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("branch failed"),
                    )
                }
            })
            .collect();
        return (records, Vec::new());
    }
    if let Some(arr) = input.as_array() {
        return parse_branch_records(arr, "array");
    }
    (Vec::new(), Vec::new())
}

/// Deserialize branch records, collecting one detail string per failed entry
/// so the caller reports the dropped branches instead of losing them silently.
fn parse_branch_records(entries: &[Value], shape: &str) -> (Vec<BranchResult>, Vec<String>) {
    let mut records = Vec::with_capacity(entries.len());
    let mut dropped = Vec::new();
    for entry in entries {
        match serde_json::from_value::<BranchResult>(entry.clone()) {
            Ok(record) => records.push(record),
            Err(e) => {
                tracing::warn!(
                    shape = %shape,
                    error = %e,
                    "fork output entry unparseable; branch dropped from merge"
                );
                dropped.push(format!("fork output entry ({shape}) unparseable: {e}"));
            }
        }
    }
    (records, dropped)
}

/// Merge branch variables into the parent scope. A `variable_outputs`
/// mapping (`{ internal_name, target_path }`) copies each branch's exported
/// variable into the parent at its target path (last-writer-wins across
/// branches); without a mapping, branch variables are not imported
/// implicitly (explicit export only).
pub fn aggregate_branch_variables(
    config: &Value,
    success_records: &[BranchResult],
    ctx: &mut NodeExecutionContext,
) -> WorkflowResult<()> {
    let Some(mappings) = config.get("variable_outputs").and_then(|v| v.as_array()) else {
        return Ok(());
    };
    for record in success_records {
        let Some(ref variables) = record.variables else {
            continue;
        };
        for mapping in mappings {
            let internal_name = mapping
                .get("internal_name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let target_path = mapping
                .get("target_path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if internal_name.is_empty() || target_path.is_empty() {
                continue;
            }
            if let Some(value) = variables.get(internal_name) {
                crate::handler::variable_mapping::set_variable_path(
                    &ctx.variables,
                    target_path,
                    value.clone(),
                )?;
            }
        }
    }
    Ok(())
}

/// Merge message contexts from the successful branches into the parent
/// scope. A `message_outputs` mapping (`{ context_id, target_context_id }`)
/// copies each branch's named message array into the parent context.
pub fn aggregate_branch_messages(ctx: &mut NodeExecutionContext, success_records: &[BranchResult]) {
    let Some(config) = &ctx.node_config else {
        return;
    };
    let Some(outputs) = config.get("message_outputs").and_then(|v| v.as_array()) else {
        return;
    };
    for record in success_records {
        let Some(ref variables) = record.variables else {
            continue;
        };
        for mapping in outputs {
            let context_id = mapping
                .get("context_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let target_context_id = mapping
                .get("target_context_id")
                .and_then(|v| v.as_str())
                .unwrap_or(context_id);
            if context_id.is_empty() || target_context_id.is_empty() {
                continue;
            }
            let key = format!("{}{}", crate::message_context::CONTEXT_PREFIX, context_id);
            if let Some(value) = variables.get(&key) {
                if let Ok(messages) =
                    serde_json::from_value::<Vec<wf_types::message::Message>>(value.clone())
                {
                    crate::message_context::append_context(
                        &ctx.variables,
                        target_context_id,
                        messages,
                    );
                }
            }
        }
    }
}

/// Merge `data_outputs` (`{ internal_name, output_key }`) from the
/// successful branches into the JOIN output: each mapping copies the named
/// branch variable into the aggregated output under `output_key`. The
/// primary record is the first successful branch (the main path); missing
/// values are skipped, so the output keeps whatever the strategy produced.
pub fn aggregate_branch_data_outputs(
    config: &Value,
    success_records: &[BranchResult],
    aggregated: &mut Value,
) {
    let Some(mappings) = config
        .get("data_outputs")
        .or_else(|| config.get("dataOutputs"))
        .and_then(|v| v.as_array())
    else {
        return;
    };
    let Some(primary) = success_records.first() else {
        return;
    };
    let Some(ref variables) = primary.variables else {
        return;
    };

    for mapping in mappings {
        let internal_name = mapping
            .get("internal_name")
            .or_else(|| mapping.get("internalName"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let output_key = mapping
            .get("output_key")
            .or_else(|| mapping.get("outputKey"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if internal_name.is_empty() || output_key.is_empty() {
            continue;
        }
        let Some(value) = variables.get(internal_name) else {
            continue;
        };
        if let Value::Object(map) = aggregated {
            map.insert(output_key.to_string(), value.clone());
        } else {
            let mut map = serde_json::Map::new();
            map.insert(output_key.to_string(), value.clone());
            *aggregated = Value::Object(map);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_collects_fork_outputs() {
        let input = serde_json::json!({
            "results": [
                {"branch_id": "b1", "output": {"x": 1}, "success": true},
                {"branch_id": "b2", "output": {"y": 2}, "success": true},
                {"branch_id": "b3", "output": {"x": 9}, "success": false, "error": "boom"}
            ]
        });
        let (records, dropped) = collect_branch_records(&input);
        assert!(dropped.is_empty());
        let outputs: Vec<Value> = records
            .iter()
            .filter(|r| r.success)
            .map(|r| r.output.clone())
            .collect();
        assert_eq!(outputs.len(), 2);
        assert_eq!(merge_outputs(&outputs), serde_json::json!({"x": 1, "y": 2}));
        assert_eq!(records[2].error.as_deref(), Some("boom"));
    }

    #[test]
    fn join_merge_concats_arrays() {
        let outputs = vec![
            serde_json::json!({"items": [1], "n": 1}),
            serde_json::json!({"items": [2, 3], "n": 2}),
        ];
        assert_eq!(
            merge_outputs(&outputs),
            serde_json::json!({"items": [1, 2, 3], "n": 2})
        );
    }
}
