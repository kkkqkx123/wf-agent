use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::checkpoint::{CheckpointListOptions, CheckpointStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::Checkpoint;

use crate::not_found;

pub async fn save_checkpoint(
    ctx: &StorageContext,
    checkpoint: &Checkpoint,
) -> crate::ApiResult<()> {
    ctx.checkpoint.save(checkpoint).await?;
    Ok(())
}

pub async fn get_checkpoint(ctx: &StorageContext, id: &str) -> crate::ApiResult<Checkpoint> {
    ctx.checkpoint
        .load(id)
        .await?
        .ok_or_else(|| not_found("checkpoint", id))
}

pub async fn delete_checkpoint(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.checkpoint.delete(id).await.map_err(Into::into)
}

pub async fn list_checkpoints(
    ctx: &StorageContext,
    options: Option<CheckpointListOptions>,
) -> crate::ApiResult<Vec<Checkpoint>> {
    ctx.checkpoint.list(options).await.map_err(Into::into)
}

pub async fn list_checkpoints_by_entity(
    ctx: &StorageContext,
    entity_id: &str,
    entity_type: &str,
) -> crate::ApiResult<Vec<Checkpoint>> {
    ctx.checkpoint
        .list_by_entity(entity_id, entity_type)
        .await
        .map_err(Into::into)
}

pub async fn get_latest_checkpoint(
    ctx: &StorageContext,
    entity_id: &str,
    entity_type: &str,
) -> crate::ApiResult<Option<Checkpoint>> {
    ctx.checkpoint
        .get_latest_by_entity(entity_id, entity_type)
        .await
        .map_err(Into::into)
}

pub async fn delete_checkpoints_by_entity(
    ctx: &StorageContext,
    entity_id: &str,
    entity_type: &str,
) -> crate::ApiResult<u64> {
    ctx.checkpoint
        .delete_by_entity(entity_id, entity_type)
        .await
        .map_err(Into::into)
}

pub async fn list_checkpoints_by_entities(
    ctx: &StorageContext,
    entity_ids: &[String],
    entity_type: &str,
) -> crate::ApiResult<Vec<Checkpoint>> {
    ctx.checkpoint
        .list_by_entities_with_metadata(entity_ids, entity_type)
        .await
        .map_err(Into::into)
}

pub async fn get_checkpoint_entity_metadata(
    ctx: &StorageContext,
    entity_id: &str,
) -> crate::ApiResult<Option<HashMap<String, Value>>> {
    ctx.checkpoint
        .get_entity_metadata(entity_id)
        .await
        .map_err(Into::into)
}

pub async fn set_checkpoint_entity_metadata(
    ctx: &StorageContext,
    entity_id: &str,
    metadata: &HashMap<String, Value>,
) -> crate::ApiResult<()> {
    ctx.checkpoint
        .set_entity_metadata(entity_id, metadata)
        .await?;
    Ok(())
}

/// One transition between consecutive checkpoints of a chain.
#[derive(Debug, Clone, Serialize)]
pub struct CheckpointTransitionView {
    pub from_checkpoint_id: String,
    pub to_checkpoint_id: String,
    pub elapsed: i64,
    /// `FULL` | `DELTA`.
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_description: Option<String>,
}

/// Time range spanned by a checkpoint chain.
#[derive(Debug, Clone, Serialize)]
pub struct CheckpointTimeRangeView {
    pub start: i64,
    pub end: i64,
}

/// Chronological checkpoint chain of an execution with transition analysis.
#[derive(Debug, Clone, Serialize)]
pub struct CheckpointChainAnalysisView {
    pub execution_id: String,
    pub checkpoints: Vec<Checkpoint>,
    pub transitions: Vec<CheckpointTransitionView>,
    pub total_elapsed: i64,
    pub checkpoint_count: usize,
    pub time_range: CheckpointTimeRangeView,
}

/// Chronological chain of an execution's checkpoints with transitions
/// between consecutive checkpoints.
pub async fn get_checkpoint_chain(
    ctx: &StorageContext,
    execution_id: &str,
) -> crate::ApiResult<CheckpointChainAnalysisView> {
    let mut sorted = list_checkpoints_by_entity(ctx, execution_id, "checkpoint").await?;
    sorted.sort_by_key(|c| c.timestamp);

    let mut transitions = Vec::new();
    for window in sorted.windows(2) {
        let prev = &window[0];
        let curr = &window[1];
        transitions.push(CheckpointTransitionView {
            from_checkpoint_id: prev.id.clone(),
            to_checkpoint_id: curr.id.clone(),
            elapsed: curr.timestamp - prev.timestamp,
            r#type: checkpoint_type_str(&curr.checkpoint_type).to_string(),
            trigger_description: curr
                .custom_fields
                .as_ref()
                .and_then(|fields| fields.get("description"))
                .and_then(|v| v.as_str())
                .map(String::from),
        });
    }

    let first = sorted.first();
    let last = sorted.last();
    Ok(CheckpointChainAnalysisView {
        execution_id: execution_id.to_string(),
        checkpoints: sorted.clone(),
        transitions,
        total_elapsed: match (first, last) {
            (Some(first), Some(last)) => last.timestamp - first.timestamp,
            _ => 0,
        },
        checkpoint_count: sorted.len(),
        time_range: CheckpointTimeRangeView {
            start: first.map(|c| c.timestamp).unwrap_or(0),
            end: last.map(|c| c.timestamp).unwrap_or(0),
        },
    })
}

/// Checkpoints of every execution of a workflow within a timestamp range,
/// oldest first.
pub async fn list_checkpoints_by_time_range(
    ctx: &StorageContext,
    workflow_id: &str,
    start: i64,
    end: i64,
) -> crate::ApiResult<Vec<Checkpoint>> {
    let executions = ctx
        .workflow_execution
        .list(Some(
            wf_storage::adapter::execution::WorkflowExecutionListOptions {
                workflow_id_filter: Some(workflow_id.to_string()),
                ..Default::default()
            },
        ))
        .await?;
    let entity_ids: Vec<String> = executions.iter().map(|e| e.id.clone()).collect();
    if entity_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut checkpoints = ctx
        .checkpoint
        .list_by_entities_with_metadata(&entity_ids, "checkpoint")
        .await?;
    checkpoints.retain(|c| c.timestamp >= start && c.timestamp <= end);
    checkpoints.sort_by_key(|c| c.timestamp);
    Ok(checkpoints)
}

fn checkpoint_type_str(
    checkpoint_type: &wf_types::checkpoint::base::CheckpointType,
) -> &'static str {
    match checkpoint_type {
        wf_types::checkpoint::base::CheckpointType::Full => "FULL",
        wf_types::checkpoint::base::CheckpointType::Delta => "DELTA",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::checkpoint::base::{CheckpointStatus, CheckpointType};

    fn make_checkpoint(id: &str, entity_id: &str, ts: i64) -> Checkpoint {
        Checkpoint {
            id: id.into(),
            entity_type: "execution".into(),
            entity_id: entity_id.into(),
            checkpoint_type: CheckpointType::Full,
            timestamp: ts,
            status: CheckpointStatus::Active,
            previous_checkpoint_id: None,
            base_checkpoint_id: None,
            chain_root_id: None,
            chain_position: None,
            blob_size: None,
            tags: None,
            custom_fields: None,
        }
    }

    #[tokio::test]
    async fn checkpoint_crud() {
        let ctx = StorageContext::new_memory();
        save_checkpoint(&ctx, &make_checkpoint("cp-1", "ex-1", 1000))
            .await
            .unwrap();

        let loaded = get_checkpoint(&ctx, "cp-1").await.unwrap();
        assert_eq!(loaded.entity_id, "ex-1");

        let err = get_checkpoint(&ctx, "cp-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_checkpoint(&ctx, "cp-1").await.unwrap());
        assert!(!delete_checkpoint(&ctx, "cp-1").await.unwrap());
    }

    #[tokio::test]
    async fn checkpoint_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_checkpoint(&ctx, &make_checkpoint("cp-1", "ex-1", 1000))
            .await
            .unwrap();
        save_checkpoint(&ctx, &make_checkpoint("cp-2", "ex-1", 3000))
            .await
            .unwrap();
        save_checkpoint(&ctx, &make_checkpoint("cp-3", "ex-2", 2000))
            .await
            .unwrap();

        // The storage adapter filters checkpoints by their own record type
        // (always "checkpoint"), combined with the checkpointed entity id.
        let by_entity = list_checkpoints_by_entity(&ctx, "ex-1", "checkpoint")
            .await
            .unwrap();
        assert_eq!(by_entity.len(), 2);

        let latest = get_latest_checkpoint(&ctx, "ex-1", "checkpoint")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.id, "cp-2");

        let multi =
            list_checkpoints_by_entities(&ctx, &["ex-1".into(), "ex-2".into()], "checkpoint")
                .await
                .unwrap();
        assert_eq!(multi.len(), 3);

        let deleted = delete_checkpoints_by_entity(&ctx, "ex-1", "checkpoint")
            .await
            .unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(list_checkpoints(&ctx, None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn checkpoint_entity_metadata() {
        let ctx = StorageContext::new_memory();
        save_checkpoint(&ctx, &make_checkpoint("cp-meta", "ex-meta", 1000))
            .await
            .unwrap();

        let mut metadata = HashMap::new();
        metadata.insert("owner".into(), Value::String("alice".into()));
        set_checkpoint_entity_metadata(&ctx, "cp-meta", &metadata)
            .await
            .unwrap();

        let stored = get_checkpoint_entity_metadata(&ctx, "cp-meta")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.get("owner").and_then(|v| v.as_str()), Some("alice"));
    }

    #[tokio::test]
    async fn checkpoint_chain_groups_and_measures() {
        let ctx = StorageContext::new_memory();
        let mut first = make_checkpoint("cp-a", "ex-chain", 1000);
        first.checkpoint_type = CheckpointType::Full;
        save_checkpoint(&ctx, &first).await.unwrap();
        let mut second = make_checkpoint("cp-b", "ex-chain", 3000);
        second.checkpoint_type = CheckpointType::Delta;
        second.previous_checkpoint_id = Some("cp-a".into());
        second.base_checkpoint_id = Some("cp-a".into());
        second.chain_root_id = Some("cp-a".into());
        second.custom_fields = Some(
            serde_json::from_value(serde_json::json!({"description": "after node n1"}))
                .expect("metadata map"),
        );
        save_checkpoint(&ctx, &second).await.unwrap();

        let chain = get_checkpoint_chain(&ctx, "ex-chain").await.unwrap();
        assert_eq!(chain.checkpoint_count, 2);
        assert_eq!(chain.total_elapsed, 2000);
        assert_eq!(chain.time_range.start, 1000);
        assert_eq!(chain.time_range.end, 3000);
        assert_eq!(chain.transitions.len(), 1);
        let transition = &chain.transitions[0];
        assert_eq!(transition.from_checkpoint_id, "cp-a");
        assert_eq!(transition.to_checkpoint_id, "cp-b");
        assert_eq!(transition.elapsed, 2000);
        assert_eq!(transition.r#type, "DELTA");
        assert_eq!(
            transition.trigger_description.as_deref(),
            Some("after node n1")
        );

        // No checkpoints -> empty chain, zeroed fields.
        let empty = get_checkpoint_chain(&ctx, "ex-none").await.unwrap();
        assert_eq!(empty.checkpoint_count, 0);
        assert_eq!(empty.total_elapsed, 0);
        assert!(empty.transitions.is_empty());
    }

    #[tokio::test]
    async fn checkpoints_by_time_range_across_executions() {
        use wf_types::WorkflowExecution;

        let ctx = StorageContext::new_memory();
        let execution = WorkflowExecution {
            id: "ex-wf-1".into(),
            workflow_id: "wf-range".into(),
            workflow_version: None,
            status: wf_types::ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: None,
            error: None,
            started_at: 0,
            completed_at: Some(4000),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.workflow_execution.save(&execution).await.unwrap();
        let unrelated = WorkflowExecution {
            id: "ex-other".into(),
            workflow_id: "wf-other".into(),
            ..execution
        };
        ctx.workflow_execution.save(&unrelated).await.unwrap();

        save_checkpoint(&ctx, &make_checkpoint("cp-1", "ex-wf-1", 1000))
            .await
            .unwrap();
        save_checkpoint(&ctx, &make_checkpoint("cp-2", "ex-wf-1", 3000))
            .await
            .unwrap();
        save_checkpoint(&ctx, &make_checkpoint("cp-3", "ex-wf-1", 5000))
            .await
            .unwrap();
        save_checkpoint(&ctx, &make_checkpoint("cp-other", "ex-other", 2000))
            .await
            .unwrap();

        // Only checkpoints of the workflow's executions within the range.
        let ranged = list_checkpoints_by_time_range(&ctx, "wf-range", 1500, 4000)
            .await
            .unwrap();
        assert_eq!(ranged.len(), 1);
        assert_eq!(ranged[0].id, "cp-2");

        // Empty window / unknown workflow.
        assert!(list_checkpoints_by_time_range(&ctx, "wf-range", 6000, 7000)
            .await
            .unwrap()
            .is_empty());
        assert!(list_checkpoints_by_time_range(&ctx, "wf-missing", 0, 10000)
            .await
            .unwrap()
            .is_empty());
    }
}
