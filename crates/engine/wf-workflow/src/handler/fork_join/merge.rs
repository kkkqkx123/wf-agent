use serde_json::Value;
use wf_execution_shared::context::NodeExecutionContext;

use crate::barrier::BranchResult;
use crate::error::WorkflowResult;

/// Merge strategy: object fields merged field-by-field (arrays concatenated,
/// scalars last-wins). Non-object outputs fall back to last-wins.
pub fn merge_outputs(outputs: &[Value]) -> Value {
    merge_outputs_with_conflicts(outputs).0
}

/// Merge like [`merge_outputs`], additionally reporting one detail string
/// per overwritten key so the caller can surface silent last-wins
/// collisions instead of losing a branch's contribution unnoticed.
/// Array concatenation is intentional merging and never reported; only
/// replacements (scalar/object overwrites, including type changes) are.
pub fn merge_outputs_with_conflicts(outputs: &[Value]) -> (Value, Vec<String>) {
    let mut merged: serde_json::Map<String, Value> = serde_json::Map::new();
    let mut conflicts = Vec::new();
    for output in outputs {
        match output {
            Value::Object(map) => {
                for (key, value) in map {
                    match (merged.get_mut(key), value) {
                        (Some(Value::Array(prev)), Value::Array(items)) => {
                            prev.extend(items.clone());
                        }
                        (Some(prev), value) => {
                            if prev != value {
                                conflicts.push(format!(
                                    "merge overwrote key '{key}'; later branch output won"
                                ));
                            }
                            *prev = value.clone();
                        }
                        (None, value) => {
                            merged.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
            value => {
                return (value.clone(), conflicts);
            }
        }
    }
    (Value::Object(merged), conflicts)
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
/// appends each branch's named message array into the parent context.
///
/// Reads the explicit `BranchResult::message_contexts` export first; the
/// legacy `variables["__msg_ctx__*"]` lookup stays as a fallback for
/// hand-built records and old `__fork_outputs_*` snapshots (which never
/// carried messages in production). Archived histories merge into the
/// parent's archived history (id-deduplicated); the parent ledger is
/// rebuilt incrementally by `append_context`.
pub fn aggregate_branch_messages(ctx: &mut NodeExecutionContext, success_records: &[BranchResult]) {
    let Some(config) = &ctx.node_config else {
        return;
    };
    let Some(outputs) = config.get("message_outputs").and_then(|v| v.as_array()) else {
        return;
    };
    for record in success_records {
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
            let value = record
                .message_contexts
                .as_ref()
                .and_then(|contexts| contexts.get(context_id))
                .or_else(|| {
                    record.variables.as_ref().and_then(|variables| {
                        variables.get(&format!(
                            "{}{}",
                            crate::message_context::CONTEXT_PREFIX,
                            context_id
                        ))
                    })
                });
            if let Some(history_value) = record
                .message_histories
                .as_ref()
                .and_then(|histories| histories.get(context_id))
            {
                match serde_json::from_value::<Vec<wf_types::message::Message>>(
                    history_value.clone(),
                ) {
                    Ok(history) => crate::message_context::merge_archived_history(
                        &ctx.variables,
                        target_context_id,
                        history,
                    ),
                    Err(e) => {
                        tracing::warn!(
                            branch_id = %record.branch_id,
                            context_id = %context_id,
                            error = %e,
                            "JOIN message history failed to parse; skipping history merge"
                        );
                        crate::degradation::emit_data_degradation(
                            ctx.event_bus.as_deref(),
                            None,
                            &ctx.execution_id,
                            "join_message_histories",
                            &format!(
                                "branch '{}' message history '{}' failed to parse: {}",
                                record.branch_id, context_id, e
                            ),
                        );
                    }
                }
            }
            let Some(value) = value else { continue };
            match serde_json::from_value::<Vec<wf_types::message::Message>>(value.clone()) {
                Ok(messages) => crate::message_context::append_context(
                    &ctx.variables,
                    target_context_id,
                    messages,
                ),
                Err(e) => {
                    tracing::warn!(
                        branch_id = %record.branch_id,
                        context_id = %context_id,
                        error = %e,
                        "JOIN message context failed to parse; skipping branch contribution"
                    );
                    crate::degradation::emit_data_degradation(
                        ctx.event_bus.as_deref(),
                        None,
                        &ctx.execution_id,
                        "join_message_outputs",
                        &format!(
                            "branch '{}' message context '{}' failed to parse: {}",
                            record.branch_id, context_id, e
                        ),
                    );
                }
            }
        }
    }
}

/// Merge `data_outputs` (`{ internal_name, output_key }`) from every
/// successful branch into the JOIN output: each mapping copies the named
/// branch variable into the aggregated output under `output_key`.
/// The `main_path_id` branch (when configured and among the successes) is
/// applied last so it wins key collisions; the remaining branches apply in
/// record order. Missing values are skipped, so the output keeps whatever
/// the strategy produced.
pub fn aggregate_branch_data_outputs(
    config: &Value,
    success_records: &[BranchResult],
    aggregated: &mut Value,
) {
    let Some(mappings) = config.get("data_outputs").and_then(|v| v.as_array()) else {
        return;
    };
    if success_records.is_empty() {
        return;
    }
    let main_path_id = config
        .get("main_path_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let mut ordered: Vec<&BranchResult> = success_records
        .iter()
        .filter(|r| r.branch_id != main_path_id)
        .collect();
    if let Some(main) = success_records.iter().find(|r| r.branch_id == main_path_id) {
        ordered.push(main);
    }

    for record in ordered {
        let Some(ref variables) = record.variables else {
            continue;
        };
        for mapping in mappings {
            let internal_name = mapping
                .get("internal_name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let output_key = mapping
                .get("output_key")
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
                {"branch_id": "b3", "output": null, "success": false, "failure": {"category": "business_failure", "detail": "boom"}}
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
        assert_eq!(records[2].error_message(), Some("boom"));
    }

    #[test]
    fn merge_reports_overwritten_keys() {
        let outputs = vec![
            serde_json::json!({"a": 1, "items": [1]}),
            serde_json::json!({"a": 2, "items": [2]}),
            serde_json::json!({"b": 3}),
        ];
        let (merged, conflicts) = merge_outputs_with_conflicts(&outputs);
        assert_eq!(merged, serde_json::json!({"a": 2, "items": [1, 2], "b": 3}));
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].contains("'a'"), "{}", conflicts[0]);

        // Identical values and array concatenation are not conflicts.
        let (_, none) = merge_outputs_with_conflicts(&[
            serde_json::json!({"a": 1}),
            serde_json::json!({"a": 1}),
        ]);
        assert!(none.is_empty());
    }

    #[test]
    fn data_outputs_merge_all_branches_with_main_priority() {
        let records = vec![
            BranchResult::success_with_variables(
                "p1",
                serde_json::json!({}),
                std::collections::HashMap::from([("v".to_string(), serde_json::json!(1))]),
            ),
            BranchResult::success_with_variables(
                "p2",
                serde_json::json!({}),
                std::collections::HashMap::from([
                    ("v".to_string(), serde_json::json!(2)),
                    ("w".to_string(), serde_json::json!(20)),
                ]),
            ),
        ];
        let config = serde_json::json!({
            "main_path_id": "p2",
            "data_outputs": [
                {"internal_name": "v", "output_key": "value"},
                {"internal_name": "w", "output_key": "extra"},
                {"internal_name": "missing", "output_key": "skipped"}
            ]
        });
        let mut aggregated = serde_json::json!({"keep": true});
        aggregate_branch_data_outputs(&config, &records, &mut aggregated);
        assert_eq!(
            aggregated,
            serde_json::json!({"keep": true, "value": 2, "extra": 20})
        );

        // Without main_path_id, record order decides (later wins).
        let config = serde_json::json!({
            "data_outputs": [{"internal_name": "v", "output_key": "value"}]
        });
        let mut aggregated = serde_json::Value::Null;
        aggregate_branch_data_outputs(&config, &records, &mut aggregated);
        assert_eq!(aggregated, serde_json::json!({"value": 2}));
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

    #[test]
    fn aggregate_messages_reads_explicit_export_and_merges_history() {
        use std::sync::Arc;
        use wf_execution_shared::context::NodeExecutionContext;
        use wf_types::node::StaticNodeType;

        let msg = |id: &str, text: &str| {
            serde_json::json!({
                "id": id,
                "role": "assistant",
                "content": text,
                "timestamp": 0,
            })
        };
        let active_b1 = vec![msg("m1", "from b1")];
        let history_b1 = vec![msg("m0", "archived b1")];
        // Old-shape record without the explicit export stays parseable and
        // contributes nothing (production never carried messages in
        // `variables`).
        let (old_records, _) = collect_branch_records(&serde_json::json!({
            "results": [{"branch_id": "old", "output": {}, "success": true}]
        }));
        assert!(old_records[0].message_contexts.is_none());

        let record = BranchResult::success("b1", serde_json::json!({})).with_message_contexts(
            std::collections::HashMap::from([(
                "chat".to_string(),
                serde_json::to_value(&active_b1).unwrap(),
            )]),
            std::collections::HashMap::from([(
                "chat".to_string(),
                serde_json::to_value(&history_b1).unwrap(),
            )]),
        );
        let vars = Arc::new(dashmap::DashMap::new());
        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "join1".to_string(),
            StaticNodeType::Join,
            serde_json::Value::Null,
            vars.clone(),
        )
        .with_node_config(serde_json::json!({
            "message_outputs": [{"context_id": "chat", "target_context_id": "merged"}]
        }));
        aggregate_branch_messages(&mut ctx, &[record]);
        let active = crate::message_context::get_context(&vars, "merged");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "m1");
        let archived = crate::message_context::archived_history(&vars, "merged");
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, "m0");
        // Parent ledger is rebuilt incrementally by the append.
        assert!(crate::message_context::ledger_estimated_tokens(&vars, "merged") > 0);
    }

    #[test]
    fn aggregate_messages_degrades_corrupted_export_without_failing() {
        use std::sync::Arc;
        use wf_execution_shared::context::NodeExecutionContext;
        use wf_types::node::StaticNodeType;

        let record = BranchResult::success("b1", serde_json::json!({})).with_message_contexts(
            std::collections::HashMap::from([(
                "chat".to_string(),
                serde_json::json!("not-a-message-array"),
            )]),
            std::collections::HashMap::new(),
        );
        let vars = Arc::new(dashmap::DashMap::new());
        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "join1".to_string(),
            StaticNodeType::Join,
            serde_json::Value::Null,
            vars.clone(),
        )
        .with_node_config(serde_json::json!({
            "message_outputs": [{"context_id": "chat", "target_context_id": "merged"}]
        }));
        aggregate_branch_messages(&mut ctx, &[record]);
        assert!(crate::message_context::get_context(&vars, "merged").is_empty());
    }

    #[test]
    fn fork_output_key_roundtrip_for_checkpoint_resume() {
        let key = super::super::fork_output_key("f1");
        assert!(key.starts_with("__"));
        assert_ne!(key, super::super::fork_output_key("f2"));

        // What the fork publishes under the key: branch results carrying
        // public variable snapshots for the JOIN fallback path.
        let published = serde_json::json!({
            "results": [
                {"branch_id": "p1", "output": {"x": 1}, "success": true,
                 "variables": {"value": 1}},
                {"branch_id": "p2", "output": {"y": 2}, "success": true,
                 "variables": {"value": 2}}
            ],
            "outputs": [
                {"branch_id": "p1", "output": {"x": 1}, "success": true},
                {"branch_id": "p2", "output": {"y": 2}, "success": true}
            ],
            "outcome": "Succeeded"
        });
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        vars.insert(key.clone(), published.clone());
        let stored = vars.get(&key).map(|v| v.clone()).expect("key readable");
        let (records, dropped) = collect_branch_records(&stored);
        assert!(dropped.is_empty(), "{dropped:?}");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].branch_id, "p1");
        assert_eq!(
            records[0].variables.as_ref().and_then(|m| m.get("value")),
            Some(&serde_json::json!(1))
        );
        let merged = merge_outputs(&records.iter().map(|r| r.output.clone()).collect::<Vec<_>>());
        assert_eq!(merged, serde_json::json!({"x": 1, "y": 2}));
    }
}
