//! Generic execution-state recorder shared by workflow and agent executions.
//!
//! The read side of execution state (`execution_state.rs`) is reconstructed
//! from live entities / persisted records. This module is the *record* side:
//! point-in-time snapshots are persisted through the [`PersistenceLayer`]
//! snapshot store, keyed per execution, so callers can answer "what was the
//! variable/call-stack/memory state at iteration N".
//!
//! One implementation serves both workflow and agent executions; each entity
//! kind provides a thin [`ExecutionStateAccessor`] adapter.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use wf_types::ExecutionStatus;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Normalized point-in-time execution state captured by an accessor.
#[derive(Debug, Clone)]
pub struct StatePoint {
    pub iteration: u32,
    pub status: ExecutionStatus,
    pub variables: BTreeMap<String, Value>,
    pub call_stack_depth: usize,
    pub memory_usage: Option<i64>,
}

/// Adapter from a concrete live execution entity to the normalized
/// [`StatePoint`] the recorder persists.
#[async_trait::async_trait]
pub trait ExecutionStateAccessor {
    async fn capture(&self) -> StatePoint;
}

/// One persisted state record of an execution (TS `recordState` output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStateRecord {
    pub execution_id: String,
    /// Monotonically increasing per-execution sequence number.
    pub sequence: u64,
    pub timestamp: i64,
    pub iteration: u32,
    pub status: String,
    pub variables: BTreeMap<String, Value>,
    pub call_stack_depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage: Option<i64>,
}

fn snapshot_prefix(execution_id: &str) -> String {
    format!("state:{execution_id}:")
}

/// Persist one state record for `execution_id`. Returns the assigned
/// sequence number (1-based).
pub async fn record_state<A: ExecutionStateAccessor>(
    ctx: &ApiContext,
    execution_id: &str,
    accessor: &A,
) -> ApiResult<u64> {
    let existing = list_state_records(ctx, execution_id).await?;
    let sequence = existing.last().map(|r| r.sequence).unwrap_or(0) + 1;

    let point = accessor.capture().await;
    let record = ExecutionStateRecord {
        execution_id: execution_id.to_string(),
        sequence,
        timestamp: wf_common::now(),
        iteration: point.iteration,
        status: point.status.as_str().to_string(),
        variables: point.variables,
        call_stack_depth: point.call_stack_depth,
        memory_usage: point.memory_usage,
    };
    ctx.persistence
        .save_snapshot(
            &format!("{}{:016}", snapshot_prefix(execution_id), sequence),
            &serde_json::to_value(&record)?,
        )
        .await?;
    Ok(sequence)
}

/// Drop all recorded state for `execution_id`.
pub async fn clear_state(ctx: &ApiContext, execution_id: &str) -> ApiResult<()> {
    ctx.persistence
        .clear_snapshots(&snapshot_prefix(execution_id))
        .await?;
    Ok(())
}

/// All state records of `execution_id`, oldest first.
pub async fn list_state_records(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ExecutionStateRecord>> {
    let snapshots = ctx
        .persistence
        .list_snapshots(&snapshot_prefix(execution_id))
        .await?;
    let mut records: Vec<ExecutionStateRecord> = Vec::with_capacity(snapshots.len());
    for (_key, value) in snapshots {
        if let Ok(record) = serde_json::from_value::<ExecutionStateRecord>(value) {
            records.push(record);
        }
    }
    records.sort_by_key(|r| r.sequence);
    Ok(records)
}

/// The latest record whose iteration is `<= iteration` (the state as of the
/// given iteration). `None` when the execution has no recorded state yet.
pub async fn get_state_at_iteration(
    ctx: &ApiContext,
    execution_id: &str,
    iteration: u32,
) -> ApiResult<Option<ExecutionStateRecord>> {
    Ok(list_state_records(ctx, execution_id)
        .await?
        .into_iter()
        .rev()
        .find(|r| r.iteration <= iteration))
}

/// The variable map as of `timestamp` (latest record at or before it).
pub async fn get_variable_snapshot(
    ctx: &ApiContext,
    execution_id: &str,
    timestamp: i64,
) -> ApiResult<Option<BTreeMap<String, Value>>> {
    Ok(list_state_records(ctx, execution_id)
        .await?
        .into_iter()
        .rev()
        .find(|r| r.timestamp <= timestamp)
        .map(|r| r.variables))
}

/// The value history of one variable: `(timestamp, value)` per state record,
/// oldest first.
pub async fn get_variable_history(
    ctx: &ApiContext,
    execution_id: &str,
    name: &str,
) -> ApiResult<Vec<(i64, Value)>> {
    Ok(list_state_records(ctx, execution_id)
        .await?
        .into_iter()
        .filter_map(|r| r.variables.get(name).cloned().map(|v| (r.timestamp, v)))
        .collect())
}

/// The `limit` variables with the most distinct values across the recorded
/// state, descending. Mirrors TS `getMostChangedVariables`.
pub async fn get_most_changed_variables(
    ctx: &ApiContext,
    execution_id: &str,
    limit: usize,
) -> ApiResult<Vec<(String, u64)>> {
    let records = list_state_records(ctx, execution_id).await?;
    let mut distinct: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for record in &records {
        for (name, value) in &record.variables {
            let serialized = serde_json::to_string(value).unwrap_or_default();
            distinct.entry(name.clone()).or_default().insert(serialized);
        }
    }
    let mut counts: Vec<(String, u64)> = distinct
        .into_iter()
        .map(|(name, values)| (name, values.len() as u64))
        .collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counts.truncate(limit);
    Ok(counts)
}

/// Total number of variable value changes observed across all state records
/// of `execution_id`.
pub async fn get_variable_mutation_count(ctx: &ApiContext, execution_id: &str) -> ApiResult<u64> {
    let records = list_state_records(ctx, execution_id).await?;
    let mut mutations = 0_u64;
    let mut previous: BTreeMap<String, Value> = BTreeMap::new();
    for record in &records {
        for (name, value) in &record.variables {
            if previous.get(name) != Some(value) {
                mutations += 1;
            }
        }
        previous = record.variables.clone();
    }
    Ok(mutations)
}

/// The call-stack depth over time of `execution_id`: `(timestamp, depth)`
/// per state record.
pub async fn get_call_stack(ctx: &ApiContext, execution_id: &str) -> ApiResult<Vec<(i64, usize)>> {
    Ok(list_state_records(ctx, execution_id)
        .await?
        .into_iter()
        .map(|r| (r.timestamp, r.call_stack_depth))
        .collect())
}

/// The most recent recorded memory usage of `execution_id` (bytes).
pub async fn get_memory_usage(ctx: &ApiContext, execution_id: &str) -> ApiResult<Option<i64>> {
    Ok(list_state_records(ctx, execution_id)
        .await?
        .into_iter()
        .rev()
        .find_map(|r| r.memory_usage))
}

/// The peak recorded memory usage of `execution_id` (bytes).
pub async fn get_peak_memory_usage(ctx: &ApiContext, execution_id: &str) -> ApiResult<Option<i64>> {
    Ok(list_state_records(ctx, execution_id)
        .await?
        .into_iter()
        .filter_map(|r| r.memory_usage)
        .max())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    use serde_json::json;

    struct StubAccessor {
        iteration: u32,
        status: ExecutionStatus,
        variables: BTreeMap<String, Value>,
        depth: usize,
        memory: Option<i64>,
    }

    #[async_trait::async_trait]
    impl ExecutionStateAccessor for StubAccessor {
        async fn capture(&self) -> StatePoint {
            StatePoint {
                iteration: self.iteration,
                status: self.status.clone(),
                variables: self.variables.clone(),
                call_stack_depth: self.depth,
                memory_usage: self.memory,
            }
        }
    }

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    fn accessor(iteration: u32, vars: &[(&str, Value)]) -> StubAccessor {
        StubAccessor {
            iteration,
            status: ExecutionStatus::Running,
            variables: vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            depth: iteration as usize,
            memory: Some(iteration as i64 * 1024),
        }
    }

    #[tokio::test]
    async fn record_and_read_back_state() {
        let ctx = make_ctx();
        let a = accessor(1, &[("x", json!(1))]);
        assert_eq!(record_state(&ctx, "e-1", &a).await.unwrap(), 1);
        let b = accessor(2, &[("x", json!(2))]);
        assert_eq!(record_state(&ctx, "e-1", &b).await.unwrap(), 2);

        let records = list_state_records(&ctx, "e-1").await.unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].sequence, 1);
        assert_eq!(records[1].variables.get("x"), Some(&json!(2)));

        let at_iter = get_state_at_iteration(&ctx, "e-1", 1)
            .await
            .unwrap()
            .expect("record");
        assert_eq!(at_iter.variables.get("x"), Some(&json!(1)));

        assert_eq!(get_memory_usage(&ctx, "e-1").await.unwrap(), Some(2 * 1024));
        assert_eq!(
            get_peak_memory_usage(&ctx, "e-1").await.unwrap(),
            Some(2 * 1024)
        );
        assert_eq!(get_variable_mutation_count(&ctx, "e-1").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn variable_analytics_over_records() {
        let ctx = make_ctx();
        record_state(&ctx, "e-2", &accessor(0, &[("x", json!(0))]))
            .await
            .unwrap();
        record_state(
            &ctx,
            "e-2",
            &accessor(1, &[("x", json!(1)), ("y", json!(1))]),
        )
        .await
        .unwrap();
        record_state(
            &ctx,
            "e-2",
            &accessor(2, &[("x", json!(1)), ("y", json!(2))]),
        )
        .await
        .unwrap();

        let history = get_variable_history(&ctx, "e-2", "x").await.unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[2].1, json!(1));

        let most = get_most_changed_variables(&ctx, "e-2", 10).await.unwrap();
        assert_eq!(most[0].0, "x");
        assert_eq!(most[0].1, 2);
        assert_eq!(most[1].1, 2);

        let depth = get_call_stack(&ctx, "e-2").await.unwrap();
        assert_eq!(depth.len(), 3);
        assert_eq!(depth[2].1, 2);
    }

    #[tokio::test]
    async fn clear_empties_execution_state() {
        let ctx = make_ctx();
        record_state(&ctx, "e-3", &accessor(1, &[("x", json!(1))]))
            .await
            .unwrap();
        clear_state(&ctx, "e-3").await.unwrap();

        assert!(list_state_records(&ctx, "e-3").await.unwrap().is_empty());
        assert!(get_memory_usage(&ctx, "e-3").await.unwrap().is_none());
    }
}
