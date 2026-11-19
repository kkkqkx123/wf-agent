//! Durable execution revisions.
//!
//! A revision row is the fast path for remote drift detection. Its sequence
//! is advanced by storage write paths; its digest is derived only from the
//! restorable projection (execution records, checkpoint blobs with their
//! metadata, and the file archive). Derived views are deliberately absent so
//! changing history, artifacts or audit data never moves a revision.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use wf_types::{ExecutionRevision, RevisionComponent};

use crate::backend::StorageBackend;
use crate::domain::store::{BatchItem, Store, StoreOperation};
use crate::error::StorageError;
use crate::util::compression::maybe_decompress;

pub const REVISION_TABLE: &str = "execution_revision";
pub const RECORD_COMPONENT: &str = "record";
pub const CHECKPOINT_COMPONENT: &str = "checkpoint";
pub const FILE_ARCHIVE_COMPONENT: &str = "file_archive";
pub const REVISION_DIGEST_VERSION: u32 = 1;

const SOURCE_EXECUTION: &str = "execution_record";
const SOURCE_CHECKPOINT: &str = "checkpoint";
const SOURCE_FILE_ARCHIVE: &str = "file_archive";
const SOURCE_RESTORE: &str = "restore";
const SOURCE_REBUILD: &str = "rebuild";

/// One change to the restorable projection accompanying a storage write.
#[derive(Debug, Clone)]
pub(crate) enum ComponentUpdate {
    Set {
        execution_id: String,
        component: RevisionComponent,
    },
    Remove {
        execution_id: String,
        kind: &'static str,
        id: String,
    },
}

fn encode_canonical<T: Serialize>(value: &T) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(StorageError::from)
}

fn decode_canonical(data: &[u8], compressed: bool) -> Result<Value, StorageError> {
    let plain = maybe_decompress(data, compressed)?;
    serde_json::from_slice(&plain).map_err(StorageError::from)
}

fn hash_text(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// Hash one JSON value after canonicalising its key order.
pub fn hash_json<T: Serialize>(value: &T) -> Result<String, StorageError> {
    Ok(hash_text(&encode_canonical(value)?))
}

/// Hash the decoded content of one stored row. Compression is a storage
/// detail and never participates in the revision digest.
pub fn hash_stored_row(data: &[u8], compressed: bool) -> Result<String, StorageError> {
    Ok(hash_text(&encode_canonical(&decode_canonical(
        data, compressed,
    )?)?))
}

/// Hash a checkpoint row from its indexed metadata and decoded blob.
pub fn hash_checkpoint_row(
    data: &[u8],
    compressed: bool,
    metadata: &Value,
) -> Result<String, StorageError> {
    let mut indexed = metadata.clone();
    if let Some(object) = indexed.as_object_mut() {
        object.remove("compressed");
    }
    let blob = decode_canonical(data, compressed)?;
    hash_json(&serde_json::json!({ "metadata": indexed, "blob": blob }))
}

/// Hash the complete file archive in path order.
pub fn hash_file_archive(files: &[(String, Vec<u8>)]) -> Result<String, StorageError> {
    let mut entries: Vec<serde_json::Value> = files
        .iter()
        .map(|(path, bytes)| serde_json::json!({ "path": path, "bytes_hex": hex::encode(bytes) }))
        .collect();
    entries.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    hash_json(&entries)
}

pub fn record_component(id: &str, value: &Value) -> Result<RevisionComponent, StorageError> {
    Ok(RevisionComponent {
        kind: RECORD_COMPONENT.to_string(),
        id: id.to_string(),
        hash: hash_json(value)?,
    })
}

pub fn checkpoint_component(
    id: &str,
    metadata: &Value,
    blob: &Value,
) -> Result<RevisionComponent, StorageError> {
    let mut indexed = metadata.clone();
    if let Some(object) = indexed.as_object_mut() {
        object.remove("compressed");
    }
    Ok(RevisionComponent {
        kind: CHECKPOINT_COMPONENT.to_string(),
        id: id.to_string(),
        hash: hash_json(&serde_json::json!({ "metadata": indexed, "blob": blob }))?,
    })
}

pub fn file_archive_component(hash: String) -> RevisionComponent {
    RevisionComponent {
        kind: FILE_ARCHIVE_COMPONENT.to_string(),
        id: FILE_ARCHIVE_COMPONENT.to_string(),
        hash,
    }
}

/// Digest of the restorable projection. Component order is normalised so
/// export order cannot change the value.
pub fn projection_digest(
    execution_id: &str,
    components: &[RevisionComponent],
) -> Result<String, StorageError> {
    let mut sorted: Vec<&RevisionComponent> = components.iter().collect();
    sorted.sort_by(|left, right| (&left.kind, &left.id).cmp(&(&right.kind, &right.id)));
    let document = serde_json::json!({
        "version": REVISION_DIGEST_VERSION,
        "execution_id": execution_id,
        "components": sorted,
    });
    hash_json(&document)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0)
}

fn encode_revision(revision: &ExecutionRevision) -> Result<BatchItem, StorageError> {
    let metadata = serde_json::json!({
        "entityType": REVISION_TABLE,
        "executionId": revision.execution_id,
        "sequence": revision.sequence,
        "updatedAt": revision.updated_at_ms,
        "source": revision.source,
        "reason": revision.reason,
        "compressed": false,
    });
    Ok(BatchItem::new(
        revision.execution_id.clone(),
        serde_json::to_vec(revision)?,
        metadata,
    ))
}

fn decode_revision(
    id: &str,
    data: &[u8],
    metadata: &Value,
) -> Result<ExecutionRevision, StorageError> {
    let compressed = metadata
        .get("compressed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let plain = maybe_decompress(data, compressed)?;
    let mut revision: ExecutionRevision = serde_json::from_slice(&plain)?;
    if revision.execution_id.is_empty() {
        revision.execution_id = id.to_string();
    }
    Ok(revision)
}

fn apply_updates(
    current: Option<ExecutionRevision>,
    execution_id: &str,
    updates: &[ComponentUpdate],
) -> Vec<RevisionComponent> {
    let mut components = current.map(|row| row.components).unwrap_or_default();
    for update in updates {
        match update {
            ComponentUpdate::Set { component, .. } => {
                components.retain(|item| item.kind != component.kind || item.id != component.id);
                components.push(component.clone());
            }
            ComponentUpdate::Remove { kind, id, .. } => {
                components.retain(|item| item.kind != *kind || item.id != *id);
            }
        }
    }
    components.sort_by(|left, right| (&left.kind, &left.id).cmp(&(&right.kind, &right.id)));
    let _ = execution_id;
    components
}

/// Owner of the revision table. Every write goes through one instance so a
/// sequence cannot be advanced independently by two callers.
#[derive(Debug, Clone)]
pub struct RevisionStore {
    backend: Arc<StorageBackend>,
}

impl RevisionStore {
    pub fn new(backend: Arc<StorageBackend>) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &Arc<StorageBackend> {
        &self.backend
    }

    pub async fn load(
        &self,
        execution_id: &str,
    ) -> Result<Option<ExecutionRevision>, StorageError> {
        let Some((data, metadata)) = self.backend.load(execution_id).await? else {
            return Ok(None);
        };
        Ok(Some(decode_revision(execution_id, &data, &metadata)?))
    }

    pub async fn list(&self) -> Result<Vec<ExecutionRevision>, StorageError> {
        let rows = self.backend.list_data(None).await?;
        rows.into_iter()
            .map(|(data, metadata)| {
                let id = metadata
                    .get("executionId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                decode_revision(&id, &data, &metadata)
            })
            .collect()
    }

    pub async fn has_rows(&self) -> Result<bool, StorageError> {
        Ok(self.backend.count(None).await? > 0)
    }

    pub async fn remove(&self, execution_id: &str) -> Result<(), StorageError> {
        self.backend.delete(execution_id).await
    }

    pub async fn clear(&self) -> Result<(), StorageError> {
        self.backend.clear().await
    }

    async fn advance_with(
        &self,
        target: &StorageBackend,
        updates: &[ComponentUpdate],
        operations: &[StoreOperation],
        source: &str,
        reason: &str,
        minimum_sequence: u64,
    ) -> Result<ExecutionRevision, StorageError> {
        let Some(first) = updates.first() else {
            return Err(StorageError::InvalidQuery(
                "revision advance has no component update".to_string(),
            ));
        };
        let execution_id = match first {
            ComponentUpdate::Set { execution_id, .. }
            | ComponentUpdate::Remove { execution_id, .. } => execution_id.clone(),
        };
        if updates.iter().any(|update| match update {
            ComponentUpdate::Set {
                execution_id: id, ..
            }
            | ComponentUpdate::Remove {
                execution_id: id, ..
            } => id != &execution_id,
        }) {
            return Err(StorageError::InvalidQuery(
                "one revision advance cannot span executions".to_string(),
            ));
        }
        let current = self.load(&execution_id).await?;
        let components = apply_updates(current.clone(), &execution_id, updates);
        let digest = projection_digest(&execution_id, &components)?;
        let revision = ExecutionRevision {
            execution_id: execution_id.clone(),
            sequence: current
                .as_ref()
                .map(|row| row.sequence)
                .unwrap_or(0)
                .max(minimum_sequence)
                .saturating_add(1),
            digest,
            updated_at_ms: now_ms(),
            source: source.to_string(),
            reason: reason.to_string(),
            components,
        };
        let revision_write = encode_revision(&revision)?;
        target
            .apply_cross_backend(
                &self.backend,
                operations,
                &[StoreOperation::Save(revision_write)],
            )
            .await?;
        Ok(revision)
    }

    pub(crate) async fn advance_components(
        &self,
        target: &StorageBackend,
        updates: &[ComponentUpdate],
        operations: &[StoreOperation],
        source: &str,
        reason: &str,
    ) -> Result<ExecutionRevision, StorageError> {
        self.advance_with(target, updates, operations, source, reason, 0)
            .await
    }

    /// Advance the file archive component after its git commit has landed.
    /// Git and the revision table cannot share a transaction; reconciliation
    /// compares the recorded digest with a fresh export to repair failures.
    pub async fn advance_file_archive(
        &self,
        execution_id: &str,
        hash: String,
    ) -> Result<ExecutionRevision, StorageError> {
        let updates = [ComponentUpdate::Set {
            execution_id: execution_id.to_string(),
            component: file_archive_component(hash),
        }];
        let current = self.load(execution_id).await?;
        let components = apply_updates(current.clone(), execution_id, &updates);
        let digest = projection_digest(execution_id, &components)?;
        let revision = ExecutionRevision {
            execution_id: execution_id.to_string(),
            sequence: current
                .as_ref()
                .map(|row| row.sequence)
                .unwrap_or(0)
                .saturating_add(1),
            digest,
            updated_at_ms: now_ms(),
            source: SOURCE_FILE_ARCHIVE.to_string(),
            reason: "archive_commit".to_string(),
            components,
        };
        let write = encode_revision(&revision)?;
        self.backend
            .save(&write.id, &write.data, &write.metadata)
            .await?;
        Ok(revision)
    }

    /// Explicit post-restore advance. The new sequence is greater than both
    /// the current local sequence and the sequence captured in the backup, so
    /// a restore can never look like a rollback.
    pub async fn advance_after_restore(
        &self,
        execution_id: &str,
        backup_sequence: u64,
    ) -> Result<ExecutionRevision, StorageError> {
        let current = self.load(execution_id).await?;
        let components = current
            .as_ref()
            .map(|row| row.components.clone())
            .unwrap_or_default();
        let digest = projection_digest(execution_id, &components)?;
        let revision = ExecutionRevision {
            execution_id: execution_id.to_string(),
            sequence: current
                .as_ref()
                .map(|row| row.sequence)
                .unwrap_or(0)
                .max(backup_sequence)
                .saturating_add(1),
            digest,
            updated_at_ms: now_ms(),
            source: SOURCE_RESTORE.to_string(),
            reason: "bundle_restore".to_string(),
            components,
        };
        let write = encode_revision(&revision)?;
        self.backend
            .save(&write.id, &write.data, &write.metadata)
            .await?;
        Ok(revision)
    }

    /// Rebuild one row from a full export after the revision table is lost.
    pub async fn rebuild(
        &self,
        execution_id: &str,
        components: Vec<RevisionComponent>,
    ) -> Result<ExecutionRevision, StorageError> {
        let digest = projection_digest(execution_id, &components)?;
        let revision = ExecutionRevision {
            execution_id: execution_id.to_string(),
            sequence: 1,
            digest,
            updated_at_ms: now_ms(),
            source: SOURCE_REBUILD.to_string(),
            reason: "full_export".to_string(),
            components,
        };
        let write = encode_revision(&revision)?;
        self.backend
            .save(&write.id, &write.data, &write.metadata)
            .await?;
        Ok(revision)
    }
}

pub(crate) fn component_from_row(
    table: &str,
    id: &str,
    data: &[u8],
    metadata: &Value,
) -> Result<Option<ComponentUpdate>, StorageError> {
    let compressed = metadata
        .get("compressed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match table {
        "execution" | "agent_execution" => Ok(Some(ComponentUpdate::Set {
            execution_id: id.to_string(),
            component: RevisionComponent {
                kind: RECORD_COMPONENT.to_string(),
                id: id.to_string(),
                hash: hash_stored_row(data, compressed)?,
            },
        })),
        "checkpoint" => {
            let Some(execution_id) = metadata
                .get("entityId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            else {
                return Ok(None);
            };
            Ok(Some(ComponentUpdate::Set {
                execution_id: execution_id.to_string(),
                component: RevisionComponent {
                    kind: CHECKPOINT_COMPONENT.to_string(),
                    id: id.to_string(),
                    hash: hash_checkpoint_row(data, compressed, metadata)?,
                },
            }))
        }
        _ => Ok(None),
    }
}

pub(crate) fn component_from_delete(
    table: &str,
    id: &str,
    metadata: &Value,
) -> Option<ComponentUpdate> {
    match table {
        "execution" | "agent_execution" => Some(ComponentUpdate::Remove {
            execution_id: id.to_string(),
            kind: RECORD_COMPONENT,
            id: id.to_string(),
        }),
        "checkpoint" => metadata
            .get("entityId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(|execution_id| ComponentUpdate::Remove {
                execution_id: execution_id.to_string(),
                kind: CHECKPOINT_COMPONENT,
                id: id.to_string(),
            }),
        _ => None,
    }
}

pub(crate) fn source_for_table(table: &str) -> &'static str {
    match table {
        "execution" | "agent_execution" => SOURCE_EXECUTION,
        "checkpoint" => SOURCE_CHECKPOINT,
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::StorageContext;
    use serde_json::json;

    #[tokio::test]
    async fn execution_writes_advance_one_sequence_per_write() {
        let ctx = StorageContext::new_memory();
        let backend = ctx.agent_execution.store();
        let data = br#"{"id":"exec-1"}"#;
        let metadata = json!({"entityType": "agent_execution", "compressed": false});

        backend.save("exec-1", data, &metadata).await.unwrap();
        let first = ctx.revisions.load("exec-1").await.unwrap().unwrap();
        assert_eq!(first.sequence, 1);
        assert_eq!(first.source, SOURCE_EXECUTION);
        assert_eq!(first.components.len(), 1);

        backend.save("exec-1", data, &metadata).await.unwrap();
        let second = ctx.revisions.load("exec-1").await.unwrap().unwrap();
        assert_eq!(second.sequence, 2);
        assert_eq!(second.digest, first.digest);
    }

    #[tokio::test]
    async fn checkpoint_writes_share_the_execution_revision() {
        let ctx = StorageContext::new_memory();
        ctx.agent_execution
            .store()
            .save(
                "exec-1",
                br#"{"id":"exec-1"}"#,
                &json!({"entityType": "agent_execution", "compressed": false}),
            )
            .await
            .unwrap();
        let before = ctx.revisions.load("exec-1").await.unwrap().unwrap();

        ctx.checkpoint
            .store()
            .save(
                "cp-1",
                br#"{"id":"cp-1"}"#,
                &json!({
                    "entityType": "checkpoint",
                    "entityId": "exec-1",
                    "compressed": false,
                }),
            )
            .await
            .unwrap();
        let after = ctx.revisions.load("exec-1").await.unwrap().unwrap();
        assert_eq!(after.sequence, before.sequence + 1);
        assert_eq!(after.source, SOURCE_CHECKPOINT);
        assert_eq!(after.components.len(), 2);
        assert_ne!(after.digest, before.digest);
    }

    #[tokio::test]
    async fn derived_views_and_missing_revision_table_are_distinct() {
        let ctx = StorageContext::new_memory();
        ctx.agent_execution
            .store()
            .save(
                "exec-1",
                br#"{"id":"exec-1"}"#,
                &json!({"entityType": "agent_execution", "compressed": false}),
            )
            .await
            .unwrap();
        let before = ctx.revisions.load("exec-1").await.unwrap().unwrap();

        ctx.message
            .store()
            .save(
                "message-1",
                br#"{"id":"message-1"}"#,
                &json!({"entityType": "message", "executionId": "exec-1", "compressed": false}),
            )
            .await
            .unwrap();
        let after = ctx.revisions.load("exec-1").await.unwrap().unwrap();
        assert_eq!(after.sequence, before.sequence);

        ctx.revisions.clear().await.unwrap();
        assert!(!ctx.revisions.has_rows().await.unwrap());
    }
}
