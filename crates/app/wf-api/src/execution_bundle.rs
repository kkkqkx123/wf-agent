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
use wf_storage::context::{AtomicOperation, EntityStoreId};
use wf_storage::domain::store::StoreOperation;
use wf_storage::revision::{
    build_restore_revision, checkpoint_component, file_archive_component, hash_file_archive,
    projection_digest, record_component,
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
    Workflow(Box<WorkflowExecution>),
    Agent(Box<AgentExecution>),
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

/// What a restore may do with a target that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RestoreMode {
    /// Create a missing target and accept one that already matches.
    #[default]
    Auto,
    /// Only create a missing target; refuse a target holding other content.
    /// A target that already matches still succeeds, so repeating a restore
    /// of the same snapshot stays idempotent.
    Create,
    /// Overwrite a target whose content differs from the snapshot.
    Replace,
}

/// The write a restore performs on its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreAction {
    Create,
    Noop,
    Replace,
}

impl RestoreAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Noop => "noop",
            Self::Replace => "replace",
        }
    }
}

/// What the target already holds, observed before any write.
#[derive(Debug, Default)]
struct TargetProbe {
    agent_record: bool,
    workflow_record: bool,
    checkpoint_ids: Vec<String>,
    archive_files: usize,
    sequence: Option<u64>,
    digest: Option<String>,
}

impl TargetProbe {
    /// A target with no record, no checkpoint row and no archived file holds
    /// nothing a restore could overwrite.
    fn is_absent(&self) -> bool {
        !self.agent_record
            && !self.workflow_record
            && self.checkpoint_ids.is_empty()
            && self.archive_files == 0
    }
}

/// Why an existing target does not match the snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DivergenceReason {
    /// Records, checkpoints or archive survived but the projection cannot be
    /// exported, so the target is a fragment rather than a whole execution.
    Partial,
    /// The target exports cleanly and its digest differs from the snapshot.
    DigestMismatch,
    /// The surviving record belongs to the other execution domain.
    ForeignDomain,
}

impl DivergenceReason {
    fn label(self) -> &'static str {
        match self {
            Self::Partial => "partial",
            Self::DigestMismatch => "mismatched",
            Self::ForeignDomain => "other-domain",
        }
    }
}

enum TargetState {
    Absent,
    Identical,
    Divergent(DivergenceReason),
}

impl TargetState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Identical => "identical",
            Self::Divergent(_) => "divergent",
        }
    }
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
    /// Revision sequence written on the execution node. Zero during a dry run
    /// because the intermediate storage writes decide the final value.
    pub remote_sequence: u64,
    /// Revision digest written on the execution node, identical to the bundle
    /// payload digest because the restore records the restored projection.
    pub remote_digest: String,
    /// State observed on the target before the restore: `absent`,
    /// `identical` or `divergent`.
    pub target_state: String,
    /// Write the restore performs: `create`, `noop` or `replace`.
    pub action: String,
    /// Sequence the target held before the restore, when it had a revision.
    pub previous_sequence: Option<u64>,
    /// Digest the target held before the restore, when it had a revision.
    pub previous_digest: Option<String>,
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
            out.push(ExecutionRecordSnapshot::Workflow(Box::new(record)));
        }
        ExecutionType::Agent | ExecutionType::Chat => {
            let record = ctx
                .storage
                .agent_execution
                .load(execution_id)
                .await?
                .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
            out.push(ExecutionRecordSnapshot::Agent(Box::new(record)));
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
                .map(serde_json::to_value)
                .transpose()
                .map_err(|e| ApiError::execution(format!("checkpoint encode failed: {e}")))?,
            "agent_loop" => agent
                .load(item.id.as_str())
                .await
                .map_err(|e| ApiError::execution(format!("agent checkpoint load failed: {e}")))?
                .map(serde_json::to_value)
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
/// row has been lost; normal inventory and backup rounds never export a
/// bundle to derive a version.
///
/// `minimum_sequence` is the sequence the caller already tracks for this
/// target. Reusing it keeps the rebuilt row on the existing comparison
/// baseline, so a rebuild does not read as a rollback.
pub async fn rebuild_revision(
    ctx: &ApiContext,
    execution_id: &str,
    minimum_sequence: u64,
) -> ApiResult<ExecutionRevision> {
    let bundle = build_bundle(ctx, execution_id, false, false).await?;
    let components = canonical_components(&bundle)?;
    ctx.storage
        .revisions
        .rebuild(execution_id, components, minimum_sequence)
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
        checkpoint: Box<wf_checkpoint::WorkflowCheckpoint>,
    },
    Agent {
        id: String,
        checkpoint: Box<wf_checkpoint::AgentCheckpoint>,
    },
    /// A checkpoint row with no state blob. Its metadata already travels in
    /// the bundle, so only the absence of a blob has to be recorded here.
    MetadataOnly,
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
                    checkpoint: Box::new(checkpoint),
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
                    checkpoint: Box::new(checkpoint),
                });
            }
            _ => out.push(DecodedCheckpoint::MetadataOnly),
        }
    }
    Ok(out)
}

fn validate_bundle(
    bundle: &ExecutionBundle,
    expected_directory: Option<&str>,
) -> ApiResult<(RestorePlan, Vec<DecodedCheckpoint>, Vec<RevisionComponent>)> {
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
    let components = canonical_components(bundle)?;
    let digest = projection_digest(&bundle.execution_id, &components)
        .map_err(|e| ApiError::execution(format!("recovery digest encode failed: {e}")))?;
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
            digest: digest.clone(),
            remote_sequence: 0,
            remote_digest: digest,
            ..Default::default()
        },
        decoded,
        components,
    ))
}

/// Validate a snapshot and converge a target onto it.
///
/// The restore is a converge operation, not an insert: after a successful
/// apply the target's restorable projection equals the snapshot's, and
/// repeating the same bundle leaves the data unchanged. A missing target is
/// therefore only one of the states handled here; a target holding different
/// content is replaced, which is the case recovery is actually needed for.
///
/// Liveness and directory boundaries are enforced instead of existence: a
/// running execution must be cancelled first, and a target owned by another
/// directory is never overwritten.
pub async fn restore_bundle(
    ctx: &ApiContext,
    bundle: &ExecutionBundle,
    expected_directory: Option<&str>,
    dry_run: bool,
    mode: RestoreMode,
) -> ApiResult<RestorePlan> {
    let (mut plan, decoded_checkpoints, components) = validate_bundle(bundle, expected_directory)?;
    if execution_is_live(ctx, &bundle.execution_id).await {
        return Err(ApiError::Conflict(format!(
            "execution '{}' is live on this node; cancel it before restoring",
            bundle.execution_id
        )));
    }
    if !bundle.file_archive.is_empty() && ctx.file_checkpoint_manager().is_none() {
        return Err(ApiError::Validation(
            "file checkpointing is disabled but the bundle carries file archive data".to_string(),
        ));
    }

    let probe = probe_target(ctx, &bundle.execution_id).await?;
    if let Some(expected) = expected_directory {
        ensure_target_directory(ctx, &bundle.execution_id, expected, &probe).await?;
    }
    let state = classify_target(ctx, bundle, &probe).await?;
    let action = match (&state, mode) {
        (TargetState::Absent, _) => RestoreAction::Create,
        (TargetState::Identical, _) => RestoreAction::Noop,
        (TargetState::Divergent(_), RestoreMode::Replace) => RestoreAction::Replace,
        (TargetState::Divergent(reason), RestoreMode::Create) => {
            return Err(ApiError::Conflict(format!(
                "execution '{}' already exists with {} content; create mode refuses to overwrite it",
                bundle.execution_id,
                reason.label()
            )))
        }
        (TargetState::Divergent(reason), RestoreMode::Auto) => {
            return Err(ApiError::Conflict(format!(
                "execution '{}' exists with {} content; retry with replace to overwrite it",
                bundle.execution_id,
                reason.label()
            )))
        }
    };

    plan.dry_run = dry_run;
    plan.target_state = state.as_str().to_string();
    plan.action = action.as_str().to_string();
    plan.previous_sequence = probe.sequence;
    plan.previous_digest = probe.digest.clone();
    if action == RestoreAction::Noop {
        // A target that already matches keeps its position: rewriting it
        // would move the sequence without changing any content.
        plan.remote_sequence = probe.sequence.unwrap_or(bundle.source_sequence);
        plan.remote_digest = probe
            .digest
            .clone()
            .unwrap_or_else(|| bundle.payload_digest.clone());
        return Ok(plan);
    }
    if dry_run {
        return Ok(plan);
    }

    // Checkpoint state blobs are written first. They are addressed by
    // checkpoint id and overwritten in place, so a retry after a later
    // failure is harmless, and a blob left without a metadata row never
    // enters the projection digest.
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
            // Metadata-only rows carry no state blob; the row itself is
            // written inside the transaction below.
            DecodedCheckpoint::MetadataOnly => {}
        }
    }

    let (revision, revision_write) = build_restore_revision(
        &bundle.execution_id,
        components,
        bundle.source_sequence,
        probe.sequence.unwrap_or(0),
    )
    .map_err(|e| ApiError::execution(format!("restore revision build failed: {e}")))?;

    // One transaction covers every row the projection is derived from, so no
    // reader observes a half-replaced execution and a failure leaves the
    // previous content untouched.
    let mut operations: Vec<AtomicOperation> = Vec::new();
    let keep: std::collections::HashSet<&str> = bundle
        .checkpoints
        .iter()
        .map(|item| item.metadata.id.as_str())
        .collect();
    for id in &probe.checkpoint_ids {
        if !keep.contains(id.as_str()) {
            operations.push(AtomicOperation::new(
                EntityStoreId::Checkpoint,
                StoreOperation::Delete(id.clone()),
            ));
        }
    }
    let mut writes_agent = false;
    let mut writes_workflow = false;
    for record in &bundle.records {
        match record {
            ExecutionRecordSnapshot::Workflow(value) => {
                writes_workflow = true;
                let row = ctx
                    .storage
                    .workflow_execution
                    .entity_store()
                    .encode_item(value)
                    .map_err(|e| ApiError::execution(format!("restore row encode failed: {e}")))?;
                operations.push(AtomicOperation::new(
                    EntityStoreId::WorkflowExecution,
                    StoreOperation::Save(row),
                ));
            }
            ExecutionRecordSnapshot::Agent(value) => {
                writes_agent = true;
                let row = ctx
                    .storage
                    .agent_execution
                    .entity_store()
                    .encode_item(value)
                    .map_err(|e| ApiError::execution(format!("restore row encode failed: {e}")))?;
                operations.push(AtomicOperation::new(
                    EntityStoreId::AgentExecution,
                    StoreOperation::Save(row),
                ));
            }
        }
    }
    // A snapshot of one domain must not leave the other domain's record
    // behind: the id would then resolve as ambiguous.
    if writes_agent && probe.workflow_record {
        operations.push(AtomicOperation::new(
            EntityStoreId::WorkflowExecution,
            StoreOperation::Delete(bundle.execution_id.clone()),
        ));
    }
    if writes_workflow && probe.agent_record {
        operations.push(AtomicOperation::new(
            EntityStoreId::AgentExecution,
            StoreOperation::Delete(bundle.execution_id.clone()),
        ));
    }
    for item in &bundle.checkpoints {
        let row = ctx
            .storage
            .checkpoint
            .entity_store()
            .encode_item(&item.metadata)
            .map_err(|e| ApiError::execution(format!("checkpoint row encode failed: {e}")))?;
        operations.push(AtomicOperation::new(
            EntityStoreId::Checkpoint,
            StoreOperation::Save(row),
        ));
    }
    ctx.storage
        .apply_atomic_with_revision(&operations, Some(revision_write))
        .await
        .map_err(|e| ApiError::execution(format!("restore transaction failed: {e}")))?;

    // The archive lives outside the database transaction. Replacing it last
    // means a failure here leaves a target whose rows are new and whose
    // archive is old, which the next comparison reports instead of hiding.
    match ctx.file_checkpoint_manager() {
        Some(manager) => {
            let files: std::collections::HashMap<String, Vec<u8>> = bundle
                .file_archive
                .iter()
                .map(decode_hex_file)
                .collect::<ApiResult<_>>()?;
            // An empty snapshot archive replaces a non-empty one too,
            // otherwise stale files survive and the export stops matching.
            manager
                .archive_replace(&bundle.execution_id, &files)
                .map_err(|e| ApiError::execution(format!("file archive replace failed: {e}")))?;
        }
        None => {
            if !bundle.file_archive.is_empty() {
                return Err(ApiError::Validation(
                    "file checkpointing is disabled".to_string(),
                ));
            }
        }
    }

    plan.remote_sequence = revision.sequence;
    plan.remote_digest = revision.digest;
    Ok(plan)
}

/// A live execution owns runtime state a restore cannot reason about, so it
/// is refused before any other check.
/// Whether the target is still executing on this node.
///
/// A registry entry alone does not mean "running": finished runs keep their
/// handle until a sweep drops it, and a deleted record can leave one behind.
/// Only an unsettled status blocks a restore.
async fn execution_is_live(ctx: &ApiContext, execution_id: &str) -> bool {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        if !entity.state.read().await.status().is_terminal() {
            return true;
        }
    }
    if let Some(entity) = ctx.agent_loop(execution_id) {
        if !entity.state.read().await.status().is_terminal() {
            return true;
        }
    }
    false
}

/// Observe what a target already holds without writing anything.
async fn probe_target(ctx: &ApiContext, execution_id: &str) -> ApiResult<TargetProbe> {
    let mut probe = TargetProbe {
        agent_record: ctx
            .storage
            .agent_execution
            .load(execution_id)
            .await?
            .is_some(),
        workflow_record: ctx
            .storage
            .workflow_execution
            .load(execution_id)
            .await?
            .is_some(),
        ..Default::default()
    };
    let mut checkpoint_ids = Vec::new();
    for entity_type in ["workflow_execution", "agent_loop"] {
        if let Ok(rows) =
            crate::checkpoint::record::list_by_entity_unified(ctx, execution_id, entity_type).await
        {
            checkpoint_ids.extend(rows.into_iter().map(|row| row.id.as_str().to_string()));
        }
    }
    probe.checkpoint_ids = checkpoint_ids;
    probe.archive_files = ctx
        .file_checkpoint_manager()
        .and_then(|manager| manager.archive_export(execution_id).ok())
        .map(|files| files.len())
        .unwrap_or(0);
    if let Ok(Some(row)) = ctx.storage.revisions.load(execution_id).await {
        probe.sequence = Some(row.sequence);
        probe.digest = Some(row.digest);
    }
    Ok(probe)
}

/// Compare the target with the snapshot. Only a target that exports cleanly
/// and matches the snapshot digest counts as identical; everything else that
/// is not empty needs a replacement.
async fn classify_target(
    ctx: &ApiContext,
    bundle: &ExecutionBundle,
    probe: &TargetProbe,
) -> ApiResult<TargetState> {
    if probe.is_absent() {
        return Ok(TargetState::Absent);
    }
    let snapshot_domain = bundle.records.first().map(|record| match record {
        ExecutionRecordSnapshot::Workflow(_) => ExecutionType::Workflow,
        ExecutionRecordSnapshot::Agent(_) => ExecutionType::Agent,
    });
    let existing_domain = if probe.agent_record {
        Some(ExecutionType::Agent)
    } else if probe.workflow_record {
        Some(ExecutionType::Workflow)
    } else {
        None
    };
    match (snapshot_domain, existing_domain) {
        (Some(snapshot), Some(existing)) if snapshot != existing => {
            return Ok(TargetState::Divergent(DivergenceReason::ForeignDomain));
        }
        // Records are gone but checkpoints or the archive survived: the
        // fragment is exactly what a restore is meant to repair.
        (_, None) => return Ok(TargetState::Divergent(DivergenceReason::Partial)),
        _ => {}
    }
    match build_bundle(ctx, &bundle.execution_id, false, false).await {
        Ok(current) if current.payload_digest == bundle.payload_digest => {
            Ok(TargetState::Identical)
        }
        Ok(_) => Ok(TargetState::Divergent(DivergenceReason::DigestMismatch)),
        Err(_) => Ok(TargetState::Divergent(DivergenceReason::Partial)),
    }
}

/// A target owned by another directory is never overwritten, even when the
/// caller is allowed to replace: the directory is a tenant boundary.
async fn ensure_target_directory(
    ctx: &ApiContext,
    execution_id: &str,
    expected: &str,
    probe: &TargetProbe,
) -> ApiResult<()> {
    let current = if probe.workflow_record {
        ctx.storage
            .workflow_execution
            .load(execution_id)
            .await?
            .and_then(|row| row.directory)
    } else if probe.agent_record {
        ctx.storage
            .agent_execution
            .load(execution_id)
            .await?
            .and_then(|row| row.directory)
    } else {
        None
    };
    if let Some(directory) = current {
        if directory != expected {
            return Err(ApiError::Validation(format!(
                "target execution '{execution_id}' belongs to another directory and cannot be replaced here"
            )));
        }
    }
    Ok(())
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
        let plan = restore_bundle(&target, &bundle, None, true, RestoreMode::Auto)
            .await
            .unwrap();
        assert!(plan.dry_run);
        assert!(crate::resolve_execution(&target, "exec-1").await.is_err());

        let applied = restore_bundle(&target, &bundle, None, false, RestoreMode::Auto)
            .await
            .unwrap();
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
        // The plan reports the position the remote actually wrote, so the
        // management side can adopt it instead of keeping a stale baseline.
        assert_eq!(applied.remote_sequence, restored_revision.sequence);
        assert_eq!(applied.remote_digest, bundle.payload_digest);
        assert_eq!(plan.remote_sequence, 0);
        assert_eq!(plan.remote_digest, bundle.payload_digest);
    }

    #[tokio::test]
    async fn rebuild_reuses_the_known_sequence() {
        let ctx = ctx();
        ctx.storage
            .agent_execution
            .save(&agent("exec-rebuild"))
            .await
            .unwrap();
        // A rebuild lands on the caller's baseline instead of a fresh low
        // number that would read as a rollback on the next comparison.
        let rebuilt = rebuild_revision(&ctx, "exec-rebuild", 9).await.unwrap();
        assert_eq!(rebuilt.sequence, 9);
        let exported = export_bundle(&ctx, "exec-rebuild", false).await.unwrap();
        assert_eq!(rebuilt.digest, exported.payload_digest);
        assert_eq!(exported.source_sequence, 9);

        let floor_one = rebuild_revision(&ctx, "exec-rebuild", 0).await.unwrap();
        assert_eq!(floor_one.sequence, 1);
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
        let err = restore_bundle(&target, &bundle, None, true, RestoreMode::Auto)
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
        let err = restore_bundle(&ctx(), &bundle, None, true, RestoreMode::Auto)
            .await
            .expect_err("tampered bundle must fail");
        assert!(err.to_string().contains("digest mismatch"));
    }

    #[tokio::test]
    async fn matching_target_restores_without_writing() {
        let source = ctx();
        source
            .storage
            .agent_execution
            .save(&agent("exec-same"))
            .await
            .unwrap();
        let bundle = export_bundle(&source, "exec-same", false).await.unwrap();
        let revision = revision_snapshot(&source, "exec-same").await.unwrap();

        // Restoring a snapshot onto the execution it came from must report a
        // no-op: bumping the sequence without changing content is exactly the
        // drift the health check reports as corruption.
        let plan = restore_bundle(&source, &bundle, None, false, RestoreMode::Replace)
            .await
            .unwrap();
        assert_eq!(plan.target_state, "identical");
        assert_eq!(plan.action, "noop");
        assert_eq!(plan.remote_sequence, revision.sequence);
        let after = revision_snapshot(&source, "exec-same").await.unwrap();
        assert_eq!(after.sequence, revision.sequence);
        assert_eq!(after.digest, revision.digest);
    }

    #[tokio::test]
    async fn mismatching_target_is_refused_without_replace() {
        let source = ctx();
        source
            .storage
            .agent_execution
            .save(&agent("exec-drift"))
            .await
            .unwrap();
        let bundle = export_bundle(&source, "exec-drift", false).await.unwrap();

        let mut drifted = agent("exec-drift");
        drifted.status = AgentExecutionStatus::Running;
        source.storage.agent_execution.save(&drifted).await.unwrap();

        let err = restore_bundle(&source, &bundle, None, true, RestoreMode::Auto)
            .await
            .expect_err("a mismatching target must not be overwritten by default");
        assert!(err.to_string().contains("retry with replace"), "{err}");
        assert!(
            restore_bundle(&source, &bundle, None, true, RestoreMode::Create)
                .await
                .is_err()
        );
        // The refused attempt changed nothing.
        let untouched = source
            .storage
            .agent_execution
            .load("exec-drift")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(untouched.status, AgentExecutionStatus::Running);
    }

    #[tokio::test]
    async fn replace_converges_a_mismatching_target() {
        let source = ctx();
        source
            .storage
            .agent_execution
            .save(&agent("exec-replace"))
            .await
            .unwrap();
        let bundle = export_bundle(&source, "exec-replace", false).await.unwrap();
        let before = revision_snapshot(&source, "exec-replace").await.unwrap();

        let mut drifted = agent("exec-replace");
        drifted.status = AgentExecutionStatus::Failed;
        source.storage.agent_execution.save(&drifted).await.unwrap();

        let plan = restore_bundle(&source, &bundle, None, true, RestoreMode::Replace)
            .await
            .unwrap();
        assert_eq!(plan.target_state, "divergent");
        assert_eq!(plan.action, "replace");
        assert_eq!(plan.previous_sequence, Some(before.sequence + 1));
        assert!(plan.previous_digest.is_some());
        assert_ne!(
            plan.previous_digest.as_deref(),
            Some(bundle.payload_digest.as_str())
        );

        let applied = restore_bundle(&source, &bundle, None, false, RestoreMode::Replace)
            .await
            .unwrap();
        assert_eq!(applied.action, "replace");
        assert!(applied.remote_sequence > before.sequence);
        let restored = source
            .storage
            .agent_execution
            .load("exec-replace")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restored.status, AgentExecutionStatus::Completed);
        // A target that was replaced exports the snapshot projection again,
        // so the next comparison sees a healthy target instead of drift.
        let reexported = export_bundle(&source, "exec-replace", false).await.unwrap();
        assert_eq!(reexported.payload_digest, bundle.payload_digest);
        assert_eq!(applied.remote_digest, bundle.payload_digest);
    }

    #[tokio::test]
    async fn partial_target_is_replaceable() {
        let source = ctx();
        source
            .storage
            .agent_execution
            .save(&agent("exec-partial"))
            .await
            .unwrap();
        let bundle = export_bundle(&source, "exec-partial", false).await.unwrap();

        // The record is gone but a checkpoint row survived: a fragment that
        // cannot be exported and that the old existence check refused.
        source
            .storage
            .agent_execution
            .delete("exec-partial")
            .await
            .unwrap();
        source
            .storage
            .checkpoint
            .save(&Checkpoint {
                id: "cp-orphan".into(),
                entity_type: "agent_loop".into(),
                entity_id: "exec-partial".into(),
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
            })
            .await
            .unwrap();

        let plan = restore_bundle(&source, &bundle, None, true, RestoreMode::Replace)
            .await
            .unwrap();
        assert_eq!(plan.target_state, "divergent");
        assert_eq!(plan.action, "replace");

        restore_bundle(&source, &bundle, None, false, RestoreMode::Replace)
            .await
            .unwrap();
        let restored = source
            .storage
            .agent_execution
            .load("exec-partial")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restored.id.as_str(), "exec-partial");
        // The orphan checkpoint row is outside the snapshot, so the replace
        // removes it; otherwise the export would keep reporting extra content.
        let exported = export_bundle(&source, "exec-partial", false).await.unwrap();
        assert_eq!(exported.payload_digest, bundle.payload_digest);
        assert!(exported.checkpoints.is_empty());
    }
}
