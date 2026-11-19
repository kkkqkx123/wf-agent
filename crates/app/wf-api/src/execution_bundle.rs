//! Execution recovery bundle export and import.
//!
//! One bundle carries the persisted execution records, unified history,
//! artifact values, audit summary, complete checkpoint blobs and the file
//! archive of one execution. The same format is used by local backups and by
//! restoration into a remote execution node. Restored data is historical
//! state for review, never a live session handle.

use serde::{Deserialize, Serialize};

use wf_checkpoint::CheckpointStateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::revision::{
    checkpoint_component, file_archive_component, hash_file_archive, projection_digest,
    record_component,
};
use wf_types::execution::ExecutionType;
use wf_types::{
    AgentExecution, Checkpoint, ExecutionRevision, RevisionComponent, WorkflowExecution,
};

use crate::audit::AuditSummary;
use crate::execution_artifacts::ArtifactEntry;
use crate::execution_history::history;
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Bundle format version. The version stays at one while the format is
/// extended in place during development.
pub const EXECUTION_BUNDLE_VERSION: u32 = 1;

/// One persisted execution record in its original typed shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionRecordSnapshot {
    Workflow(WorkflowExecution),
    Agent(AgentExecution),
}

impl ExecutionRecordSnapshot {
    pub fn execution_id(&self) -> &str {
        match self {
            Self::Workflow(record) => record.id.as_str(),
            Self::Agent(record) => record.id.as_str(),
        }
    }

    fn directory(&self) -> Option<&str> {
        match self {
            Self::Workflow(record) => record.directory.as_deref(),
            Self::Agent(record) => record.directory.as_deref(),
        }
    }
}

/// One checkpoint with the state blob its metadata points at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRecovery {
    pub metadata: Checkpoint,
    pub blob: Option<serde_json::Value>,
}

/// One captured file of the execution context archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileArchiveEntry {
    pub path: String,
    pub bytes_hex: String,
}

/// Portable, self-contained recovery snapshot of one execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionBundle {
    pub version: u32,
    pub execution_id: String,
    pub exported_at: i64,
    /// Durable sequence captured from the revision row at export time.
    pub source_sequence: u64,
    /// Digest of the restorable projection. Export-time fields and derived
    /// views are excluded so a restored execution recomputes the same value.
    pub source_revision: String,
    pub payload_digest: String,
    pub records: Vec<ExecutionRecordSnapshot>,
    pub history: serde_json::Value,
    pub artifacts: Vec<ArtifactEntry>,
    pub audit: AuditSummary,
    pub checkpoints: Vec<CheckpointRecovery>,
    pub file_archive: Vec<FileArchiveEntry>,
    pub checkpoint_synced: bool,
}

/// Counts reported by a restore plan or an applied restore.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RestorePlan {
    pub execution_id: String,
    pub dry_run: bool,
    pub records: usize,
    pub checkpoints: usize,
    pub file_archive: usize,
    pub digest: String,
}

fn canonical_components(bundle: &ExecutionBundle) -> ApiResult<Vec<RevisionComponent>> {
    let mut components = Vec::with_capacity(
        bundle.records.len()
            + bundle.checkpoints.len()
            + usize::from(!bundle.file_archive.is_empty()),
    );
    for record in &bundle.records {
        let record_id = record.execution_id();
        let value = match record {
            ExecutionRecordSnapshot::Workflow(value) => serde_json::to_value(value),
            ExecutionRecordSnapshot::Agent(value) => serde_json::to_value(value),
        }
        .map_err(|e| ApiError::execution(format!("recovery record encode failed: {e}")))?;
        components.push(
            record_component(record_id, &value)
                .map_err(|e| ApiError::execution(format!("recovery record hash failed: {e}")))?,
        );
    }
    for item in &bundle.checkpoints {
        let blob = item.blob.clone().unwrap_or(serde_json::Value::Null);
        components.push(
            checkpoint_component(
                item.metadata.id.as_str(),
                &item.metadata.metadata_document(),
                &blob,
            )
            .map_err(|e| ApiError::execution(format!("checkpoint hash failed: {e}")))?,
        );
    }
    if !bundle.file_archive.is_empty() {
        let files: Vec<(String, Vec<u8>)> = bundle
            .file_archive
            .iter()
            .map(decode_hex_file)
            .collect::<ApiResult<_>>()?;
        let hash = hash_file_archive(&files)
            .map_err(|e| ApiError::execution(format!("file archive hash failed: {e}")))?;
        components.push(file_archive_component(hash));
    }
    Ok(components)
}

fn canonical_digest(bundle: &ExecutionBundle) -> ApiResult<String> {
    projection_digest(&bundle.execution_id, &canonical_components(bundle)?)
        .map_err(|e| ApiError::execution(format!("recovery digest encode failed: {e}")))
}

async fn export_records(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ExecutionRecordSnapshot>> {
    let domain = crate::execution_hierarchy::execution_type(ctx, execution_id).await?;
    let mut out = Vec::new();
    match domain {
        ExecutionType::Workflow => {
            let record = ctx
                .storage
                .workflow_execution
                .load(execution_id)
                .await?
                .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
            out.push(ExecutionRecordSnapshot::Workflow(record));
        }
        ExecutionType::Agent | ExecutionType::Chat => {
            let record = ctx
                .storage
                .agent_execution
                .load(execution_id)
                .await?
                .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
            out.push(ExecutionRecordSnapshot::Agent(record));
        }
    }
    Ok(out)
}

async fn export_checkpoints(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<CheckpointRecovery>> {
    let metadata = crate::checkpoint::record::list_for_execution(ctx, execution_id, None).await?;
    let workflow = wf_checkpoint::WorkflowCheckpointStateManager::new(ctx.checkpoint_store.clone());
    let agent = wf_checkpoint::AgentCheckpointStateManager::new(ctx.checkpoint_store.clone());
    let mut out = Vec::with_capacity(metadata.len());
    for item in metadata {
        let blob = match item.entity_type.as_str() {
            "workflow_execution" => workflow
                .load(item.id.as_str())
                .await
                .map_err(|e| ApiError::execution(format!("workflow checkpoint load failed: {e}")))?
                .map(|cp| serde_json::to_value(cp))
                .transpose()
                .map_err(|e| ApiError::execution(format!("checkpoint encode failed: {e}")))?,
            "agent_loop" => agent
                .load(item.id.as_str())
                .await
                .map_err(|e| ApiError::execution(format!("agent checkpoint load failed: {e}")))?
                .map(|cp| serde_json::to_value(cp))
                .transpose()
                .map_err(|e| ApiError::execution(format!("checkpoint encode failed: {e}")))?,
            _ => None,
        };
        out.push(CheckpointRecovery {
            metadata: item,
            blob,
        });
    }
    Ok(out)
}

fn export_file_archive(ctx: &ApiContext, execution_id: &str) -> ApiResult<Vec<FileArchiveEntry>> {
    let Some(manager) = ctx.file_checkpoint_manager() else {
        return Ok(Vec::new());
    };
    let files = manager
        .archive_export(execution_id)
        .map_err(|e| ApiError::execution(format!("file archive export failed: {e}")))?;
    Ok(files
        .into_iter()
        .map(|(path, bytes)| FileArchiveEntry {
            path,
            bytes_hex: hex::encode(bytes),
        })
        .collect())
}

async fn build_bundle(
    ctx: &ApiContext,
    execution_id: &str,
    checkpoint_synced: bool,
    require_revision: bool,
) -> ApiResult<ExecutionBundle> {
    let records = export_records(ctx, execution_id).await?;
    let history_value = serde_json::to_value(
        history(
            ctx,
            execution_id,
            &crate::execution_history::ExecutionHistorySections::all(),
        )
        .await?,
    )
    .map_err(|e| ApiError::execution(format!("history encode failed: {e}")))?;
    let artifacts = crate::execution_artifacts::artifacts_for_execution(ctx, execution_id).await?;
    let audit = crate::audit::audit_summary(ctx, execution_id).await?;
    let checkpoints = export_checkpoints(ctx, execution_id).await?;
    let file_archive = export_file_archive(ctx, execution_id)?;
    let revision = revision_snapshot(ctx, execution_id).await.ok();
    if require_revision && revision.is_none() {
        return Err(ApiError::execution(format!(
            "execution revision is missing for '{execution_id}'"
        )));
    }
    let mut bundle = ExecutionBundle {
        version: EXECUTION_BUNDLE_VERSION,
        execution_id: execution_id.to_string(),
        exported_at: wf_common::now(),
        source_sequence: revision.map(|row| row.sequence).unwrap_or(0),
        source_revision: String::new(),
        payload_digest: String::new(),
        records,
        history: history_value,
        artifacts,
        audit,
        checkpoints,
        file_archive,
        checkpoint_synced,
    };
    let digest = canonical_digest(&bundle)?;
    bundle.source_revision = digest.clone();
    bundle.payload_digest = digest;
    Ok(bundle)
}

/// Export one execution into a self-contained recovery snapshot.
pub async fn export_bundle(
    ctx: &ApiContext,
    execution_id: &str,
    checkpoint_synced: bool,
) -> ApiResult<ExecutionBundle> {
    build_bundle(ctx, execution_id, checkpoint_synced, true).await
}

/// Rebuild one revision row from a full export. Used only when the revision
/// table has been lost; normal inventory and backup rounds never export a
/// bundle to derive a version.
pub async fn rebuild_revision(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<ExecutionRevision> {
    let bundle = build_bundle(ctx, execution_id, false, false).await?;
    let components = canonical_components(&bundle)?;
    ctx.storage
        .revisions
        .rebuild(execution_id, components)
        .await
        .map_err(|e| ApiError::execution(format!("execution revision rebuild failed: {e}")))
}

/// Durable revision row of one execution. Inventory reads this directly so
/// polling never exports a bundle to derive a version.
pub async fn revision_snapshot(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<ExecutionRevision> {
    ctx.storage
        .revisions
        .load(execution_id)
        .await
        .map_err(|e| ApiError::execution(format!("execution revision load failed: {e}")))?
        .ok_or_else(|| {
            ApiError::execution(format!(
                "execution revision is missing for '{execution_id}'"
            ))
        })
}

fn decode_hex_file(entry: &FileArchiveEntry) -> ApiResult<(String, Vec<u8>)> {
    if entry.path.starts_with('/') || entry.path.split('/').any(|part| part == "..") {
        return Err(ApiError::Validation(format!(
            "unsafe file archive path: {}",
            entry.path
        )));
    }
    let bytes = hex::decode(&entry.bytes_hex)
        .map_err(|e| ApiError::Validation(format!("invalid file archive bytes: {e}")))?;
    Ok((entry.path.clone(), bytes))
}

enum DecodedCheckpoint {
    Workflow {
        id: String,
        checkpoint: wf_checkpoint::WorkflowCheckpoint,
    },
    Agent {
        id: String,
        checkpoint: wf_checkpoint::AgentCheckpoint,
    },
    Metadata(Checkpoint),
}

fn decode_checkpoints(bundle: &ExecutionBundle) -> ApiResult<Vec<DecodedCheckpoint>> {
    let mut out = Vec::with_capacity(bundle.checkpoints.len());
    for item in &bundle.checkpoints {
        match item.metadata.entity_type.as_str() {
            "workflow_execution" => {
                let value = item.blob.clone().ok_or_else(|| {
                    ApiError::Validation(format!(
                        "workflow checkpoint blob missing: {}",
                        item.metadata.id
                    ))
                })?;
                let checkpoint = serde_json::from_value(value).map_err(|e| {
                    ApiError::Validation(format!(
                        "workflow checkpoint decode failed: {}: {e}",
                        item.metadata.id
                    ))
                })?;
                out.push(DecodedCheckpoint::Workflow {
                    id: item.metadata.id.clone(),
                    checkpoint,
                });
            }
            "agent_loop" => {
                let value = item.blob.clone().ok_or_else(|| {
                    ApiError::Validation(format!(
                        "agent checkpoint blob missing: {}",
                        item.metadata.id
                    ))
                })?;
                let checkpoint = serde_json::from_value(value).map_err(|e| {
                    ApiError::Validation(format!(
                        "agent checkpoint decode failed: {}: {e}",
                        item.metadata.id
                    ))
                })?;
                out.push(DecodedCheckpoint::Agent {
                    id: item.metadata.id.clone(),
                    checkpoint,
                });
            }
            _ => out.push(DecodedCheckpoint::Metadata(item.metadata.clone())),
        }
    }
    Ok(out)
}

fn validate_bundle(
    bundle: &ExecutionBundle,
    expected_directory: Option<&str>,
) -> ApiResult<(RestorePlan, Vec<DecodedCheckpoint>)> {
    if bundle.version != EXECUTION_BUNDLE_VERSION {
        return Err(ApiError::Validation(format!(
            "unsupported recovery bundle version {}",
            bundle.version
        )));
    }
    if bundle.execution_id.trim().is_empty() {
        return Err(ApiError::Validation(
            "recovery bundle has no execution id".to_string(),
        ));
    }
    let digest = canonical_digest(bundle)?;
    if bundle.source_sequence == 0 {
        return Err(ApiError::Validation(
            "recovery bundle has no source sequence".to_string(),
        ));
    }
    if bundle.payload_digest != digest || bundle.source_revision != digest {
        return Err(ApiError::Validation(format!(
            "recovery bundle digest mismatch for '{}'",
            bundle.execution_id
        )));
    }
    if bundle.records.is_empty() {
        return Err(ApiError::Validation(
            "recovery bundle has no execution records".to_string(),
        ));
    }
    for record in &bundle.records {
        if record.execution_id() != bundle.execution_id {
            return Err(ApiError::Validation(format!(
                "recovery record execution mismatch: expected '{}', got '{}'",
                bundle.execution_id,
                record.execution_id()
            )));
        }
        if let Some(expected) = expected_directory {
            if record.directory() != Some(expected) {
                return Err(ApiError::Validation(format!(
                    "recovery record directory mismatch for '{}'",
                    bundle.execution_id
                )));
            }
        }
    }
    for item in &bundle.checkpoints {
        if item.metadata.entity_id != bundle.execution_id {
            return Err(ApiError::Validation(format!(
                "checkpoint execution mismatch: {}",
                item.metadata.id
            )));
        }
    }
    for file in &bundle.file_archive {
        decode_hex_file(file)?;
    }
    let decoded = decode_checkpoints(bundle)?;
    Ok((
        RestorePlan {
            execution_id: bundle.execution_id.clone(),
            dry_run: true,
            records: bundle.records.len(),
            checkpoints: bundle.checkpoints.len(),
            file_archive: bundle.file_archive.len(),
            digest,
        },
        decoded,
    ))
}

/// Validate a snapshot and restore it into a target that does not exist yet.
/// Dry runs perform every check without writing anything.
pub async fn restore_bundle(
    ctx: &ApiContext,
    bundle: &ExecutionBundle,
    expected_directory: Option<&str>,
    dry_run: bool,
) -> ApiResult<RestorePlan> {
    let (mut plan, decoded_checkpoints) = validate_bundle(bundle, expected_directory)?;
    if crate::resolve_execution(ctx, &bundle.execution_id)
        .await
        .is_ok()
    {
        return Err(ApiError::AlreadyExists {
            entity_type: "execution".to_string(),
            id: bundle.execution_id.clone(),
        });
    }
    if !bundle.file_archive.is_empty() && ctx.file_checkpoint_manager().is_none() {
        return Err(ApiError::Validation(
            "file checkpointing is disabled but the bundle carries file archive data".to_string(),
        ));
    }
    plan.dry_run = dry_run;
    if dry_run {
        return Ok(plan);
    }

    for record in &bundle.records {
        match record {
            ExecutionRecordSnapshot::Workflow(value) => {
                ctx.storage.workflow_execution.save(value).await?;
            }
            ExecutionRecordSnapshot::Agent(value) => {
                ctx.storage.agent_execution.save(value).await?;
            }
        }
    }

    let workflow = wf_checkpoint::WorkflowCheckpointStateManager::new(ctx.checkpoint_store.clone());
    let agent = wf_checkpoint::AgentCheckpointStateManager::new(ctx.checkpoint_store.clone());
    for decoded in decoded_checkpoints {
        match decoded {
            DecodedCheckpoint::Workflow { id, checkpoint } => {
                workflow
                    .save(&checkpoint, "workflow_execution", &bundle.execution_id)
                    .await
                    .map_err(|e| {
                        ApiError::execution(format!("workflow checkpoint save failed: {id}: {e}"))
                    })?;
            }
            DecodedCheckpoint::Agent { id, checkpoint } => {
                agent
                    .save(&checkpoint, "agent_loop", &bundle.execution_id)
                    .await
                    .map_err(|e| {
                        ApiError::execution(format!("agent checkpoint save failed: {id}: {e}"))
                    })?;
            }
            DecodedCheckpoint::Metadata(metadata) => {
                ctx.storage.checkpoint.save(&metadata).await?;
            }
        }
    }

    if !bundle.file_archive.is_empty() {
        let files: Vec<(String, Vec<u8>)> = bundle
            .file_archive
            .iter()
            .map(decode_hex_file)
            .collect::<ApiResult<_>>()?;
        ctx.file_checkpoint_manager()
            .ok_or_else(|| ApiError::Validation("file checkpointing is disabled".to_string()))?
            .archive_import(&bundle.execution_id, &files)
            .map_err(|e| ApiError::execution(format!("file archive import failed: {e}")))?;
    }

    ctx.storage
        .revisions
        .advance_after_restore(&bundle.execution_id, bundle.source_sequence)
        .await
        .map_err(|e| ApiError::execution(format!("revision restore failed: {e}")))?;

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;
    use wf_types::agent_execution::AgentExecutionStatus;
    use wf_types::checkpoint::{CheckpointStatus, CheckpointType};
    use wf_types::execution::ExecutionType;
    use wf_types::{Checkpoint, Id};

    fn ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    fn agent(id: &str) -> AgentExecution {
        AgentExecution {
            id: Id::from(id.to_string()),
            definition_id: Id::from("agent-1".to_string()),
            execution_type: ExecutionType::Agent,
            status: AgentExecutionStatus::Completed,
            current_iteration: 1,
            tool_call_count: 0,
            iteration_history: None,
            started_at: 1,
            completed_at: Some(30),
            error: None,
            context: None,
            loop_config: None,
            permanently_failed_tools: None,
            hierarchy: None,
            rerun_of: None,
            directory: None,
        }
    }

    #[tokio::test]
    async fn unknown_execution_export_fails_not_found() {
        let ctx = ctx();
        let err = export_bundle(&ctx, "missing-bundle", false)
            .await
            .expect_err("unknown execution must not export");
        assert!(err.to_string().contains("missing-bundle"));
    }

    #[tokio::test]
    async fn bundle_round_trips_into_missing_execution() {
        let source = ctx();
        source
            .storage
            .agent_execution
            .save(&agent("exec-1"))
            .await
            .unwrap();
        let bundle = export_bundle(&source, "exec-1", false).await.unwrap();
        let revision = revision_snapshot(&source, "exec-1").await.unwrap();
        assert_eq!(bundle.source_sequence, revision.sequence);
        assert_eq!(bundle.source_revision, bundle.payload_digest);
        assert_eq!(revision.digest, bundle.payload_digest);
        assert_eq!(bundle.records.len(), 1);

        let target = ctx();
        let plan = restore_bundle(&target, &bundle, None, true).await.unwrap();
        assert!(plan.dry_run);
        assert!(crate::resolve_execution(&target, "exec-1").await.is_err());

        let applied = restore_bundle(&target, &bundle, None, false).await.unwrap();
        assert!(!applied.dry_run);
        let restored = target
            .storage
            .agent_execution
            .load("exec-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restored.id.as_str(), "exec-1");
        let restored_revision = revision_snapshot(&target, "exec-1").await.unwrap();
        assert!(restored_revision.sequence > bundle.source_sequence);
        assert_eq!(restored_revision.source, "restore");
        assert_eq!(restored_revision.digest, bundle.payload_digest);
    }

    #[tokio::test]
    async fn malformed_checkpoint_blob_fails_before_writes() {
        let source = ctx();
        source
            .storage
            .agent_execution
            .save(&agent("exec-bad-cp"))
            .await
            .unwrap();
        let mut bundle = export_bundle(&source, "exec-bad-cp", false).await.unwrap();
        bundle.checkpoints.push(CheckpointRecovery {
            metadata: Checkpoint {
                id: "cp-bad".into(),
                entity_type: "workflow_execution".into(),
                entity_id: "exec-bad-cp".into(),
                parent_entity_id: None,
                checkpoint_type: CheckpointType::Full,
                timestamp: 1,
                status: CheckpointStatus::Active,
                previous_checkpoint_id: None,
                base_checkpoint_id: None,
                chain_root_id: None,
                chain_position: None,
                blob_size: None,
                tags: None,
                custom_fields: None,
            },
            blob: Some(serde_json::json!({"invalid": true})),
        });
        let digest = canonical_digest(&bundle).unwrap();
        bundle.source_revision = digest.clone();
        bundle.payload_digest = digest;

        let target = ctx();
        let err = restore_bundle(&target, &bundle, None, true)
            .await
            .expect_err("bad checkpoint blob must fail in preflight");
        assert!(err
            .to_string()
            .contains("workflow checkpoint decode failed"));
        assert!(crate::resolve_execution(&target, "exec-bad-cp")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn derived_views_do_not_change_the_source_revision() {
        let source = ctx();
        source
            .storage
            .agent_execution
            .save(&agent("exec-derived"))
            .await
            .unwrap();
        let bundle = export_bundle(&source, "exec-derived", false).await.unwrap();
        let before = bundle.source_revision.clone();
        let mut changed = bundle;
        changed.history = serde_json::json!({"view": "new"});
        changed.exported_at += 1;
        assert_eq!(canonical_digest(&changed).unwrap(), before);
    }

    #[tokio::test]
    async fn tampered_bundle_is_rejected() {
        let source = ctx();
        source
            .storage
            .agent_execution
            .save(&agent("exec-2"))
            .await
            .unwrap();
        let mut bundle = export_bundle(&source, "exec-2", false).await.unwrap();
        bundle.records.clear();
        let err = restore_bundle(&ctx(), &bundle, None, true)
            .await
            .expect_err("tampered bundle must fail");
        assert!(err.to_string().contains("digest mismatch"));
    }
}
