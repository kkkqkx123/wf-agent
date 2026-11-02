use checkpoint_base::delta::CheckpointLoader;
use checkpoint_base::error::CheckpointError;
use checkpoint_base::serializer::CheckpointSerializer;
use checkpoint_base::version_manager::VersionManager;
use checkpoint_state::state::CheckpointStateManager;
use wf_types::checkpoint::BaseCheckpointCore;

/// Format version stamped on a stored checkpoint blob. Both checkpoint types
/// share the same core shape, so one blanket implementation covers them.
pub trait CheckpointVersion: Send + Sync {
    fn format_version(&self) -> Option<&str>;
}

impl<TDelta, TSnapshot> CheckpointVersion for BaseCheckpointCore<TDelta, TSnapshot>
where
    TDelta: Send + Sync,
    TSnapshot: Send + Sync,
{
    fn format_version(&self) -> Option<&str> {
        self.format_version.as_deref()
    }
}

/// Load a checkpoint blob and bring it to the current format version. Rows
/// at the current version are returned as loaded; incompatible versions fail
/// with `VersionIncompatible`; migration re-reads the raw bytes because
/// migration handlers rewrite the blob.
pub async fn load_migrated<M>(
    manager: &M,
    version_manager: &VersionManager,
    checkpoint_id: &str,
) -> Result<M::Checkpoint, CheckpointError>
where
    M: CheckpointStateManager + CheckpointLoader,
    M::Checkpoint: CheckpointVersion + serde::de::DeserializeOwned,
{
    let checkpoint =
        manager
            .load(checkpoint_id)
            .await?
            .ok_or_else(|| CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            })?;

    let version = checkpoint
        .format_version()
        .ok_or_else(|| CheckpointError::Validation {
            reason: "checkpoint missing format version".to_string(),
        })?;

    let compatibility = version_manager.check_compatibility(version);
    if !compatibility.compatible {
        return Err(CheckpointError::VersionIncompatible {
            current: version_manager.current_version().to_string(),
            required: version.to_string(),
        });
    }

    if !compatibility.requires_migration {
        return Ok(checkpoint);
    }

    let raw = manager
        .load_checkpoint_data(checkpoint_id)
        .await?
        .ok_or_else(|| CheckpointError::NotFound {
            id: checkpoint_id.to_string(),
        })?;
    let raw = CheckpointSerializer::decompressed(&raw)?;
    let migrated = version_manager.migrate_data(&raw, version).await?;
    CheckpointSerializer::auto_deserialize(&migrated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct VersionedCheckpoint {
        id: String,
        format_version: Option<String>,
    }

    impl CheckpointVersion for VersionedCheckpoint {
        fn format_version(&self) -> Option<&str> {
            self.format_version.as_deref()
        }
    }

    struct StubManager {
        checkpoints: HashMap<String, (VersionedCheckpoint, Vec<u8>)>,
    }

    impl StubManager {
        fn empty() -> Self {
            Self {
                checkpoints: HashMap::new(),
            }
        }

        fn insert(&mut self, checkpoint: VersionedCheckpoint, raw: Vec<u8>) {
            self.checkpoints
                .insert(checkpoint.id.clone(), (checkpoint, raw));
        }

        fn stored(id: &str, version: Option<&str>) -> (VersionedCheckpoint, Vec<u8>) {
            let checkpoint = VersionedCheckpoint {
                id: id.to_string(),
                format_version: version.map(str::to_string),
            };
            let raw = serde_json::to_vec(&checkpoint).expect("test checkpoint serializes");
            (checkpoint, raw)
        }
    }

    impl CheckpointStateManager for StubManager {
        type Checkpoint = VersionedCheckpoint;

        async fn save(
            &self,
            _checkpoint: &VersionedCheckpoint,
            _entity_type: &str,
            _entity_id: &str,
        ) -> Result<(), CheckpointError> {
            Ok(())
        }

        async fn load(&self, id: &str) -> Result<Option<VersionedCheckpoint>, CheckpointError> {
            Ok(self
                .checkpoints
                .get(id)
                .map(|(checkpoint, _)| checkpoint.clone()))
        }

        async fn load_batch(
            &self,
            ids: &[String],
        ) -> Result<Vec<Option<VersionedCheckpoint>>, CheckpointError> {
            Ok(ids
                .iter()
                .map(|id| {
                    self.checkpoints
                        .get(id)
                        .map(|(checkpoint, _)| checkpoint.clone())
                })
                .collect())
        }

        async fn delete(&self, _id: &str) -> Result<bool, CheckpointError> {
            Ok(false)
        }

        async fn list_by_entity(
            &self,
            _entity_id: &str,
        ) -> Result<Vec<wf_types::storage::CheckpointStorageMetadata>, CheckpointError> {
            Ok(Vec::new())
        }

        async fn get_latest(
            &self,
            _entity_id: &str,
        ) -> Result<Option<wf_types::storage::CheckpointStorageMetadata>, CheckpointError> {
            Ok(None)
        }

        async fn load_metadata(
            &self,
            _id: &str,
        ) -> Result<Option<wf_types::storage::CheckpointStorageMetadata>, CheckpointError> {
            Ok(None)
        }

        async fn cleanup(
            &self,
            _entity_id: &str,
            _max_count: Option<u32>,
        ) -> Result<u64, CheckpointError> {
            Ok(0)
        }
    }

    #[async_trait::async_trait]
    impl CheckpointLoader for StubManager {
        async fn load_checkpoint_data(&self, id: &str) -> Result<Option<Vec<u8>>, CheckpointError> {
            Ok(self.checkpoints.get(id).map(|(_, raw)| raw.clone()))
        }

        async fn load_metadata(
            &self,
            _id: &str,
        ) -> Result<Option<wf_types::storage::CheckpointStorageMetadata>, CheckpointError> {
            Ok(None)
        }
    }

    struct MarkHandler {
        called: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl checkpoint_base::version_manager::MigrationHandler for MarkHandler {
        async fn migrate(
            &self,
            data: &[u8],
            _from: &str,
            _to: &str,
        ) -> Result<Vec<u8>, CheckpointError> {
            self.called.store(true, Ordering::SeqCst);
            Ok(data.to_vec())
        }
    }

    #[tokio::test]
    async fn unknown_id_reports_not_found() {
        let manager = StubManager::empty();
        let err = load_migrated(&manager, &VersionManager::new(), "nope")
            .await
            .unwrap_err();
        assert!(matches!(err, CheckpointError::NotFound { .. }));
    }

    #[tokio::test]
    async fn missing_format_version_is_rejected() {
        let mut manager = StubManager::empty();
        let (checkpoint, raw) = StubManager::stored("cp-1", None);
        manager.insert(checkpoint, raw);
        let err = load_migrated(&manager, &VersionManager::new(), "cp-1")
            .await
            .unwrap_err();
        assert!(
            matches!(err, CheckpointError::Validation { .. }),
            "version-less rows fail loudly instead of loading blind: {err:?}"
        );
    }

    #[tokio::test]
    async fn future_version_is_rejected_as_incompatible() {
        let mut manager = StubManager::empty();
        let (checkpoint, raw) = StubManager::stored("cp-1", Some("99"));
        manager.insert(checkpoint, raw);
        let err = load_migrated(&manager, &VersionManager::new(), "cp-1")
            .await
            .unwrap_err();
        match err {
            CheckpointError::VersionIncompatible { current, required } => {
                assert_eq!(current, "1");
                assert_eq!(required, "99");
            }
            other => panic!("expected VersionIncompatible, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn current_version_returns_stored_without_reading_raw() {
        let mut manager = StubManager::empty();
        let checkpoint = VersionedCheckpoint {
            id: "cp-1".to_string(),
            format_version: Some("1".to_string()),
        };
        manager.insert(checkpoint, b"corrupt-not-json".to_vec());
        let loaded = load_migrated(&manager, &VersionManager::new(), "cp-1")
            .await
            .unwrap();
        assert_eq!(loaded.id, "cp-1");
    }

    #[tokio::test]
    async fn registered_migration_re_reads_raw_bytes() {
        let called = Arc::new(AtomicBool::new(false));
        let mut version_manager = VersionManager::new();
        version_manager.set_current_version("2").unwrap();
        version_manager.register_migration(
            "1",
            "2",
            Box::new(MarkHandler {
                called: Arc::clone(&called),
            }),
        );
        let mut manager = StubManager::empty();
        let stored = VersionedCheckpoint {
            id: "cp-1".to_string(),
            format_version: Some("1".to_string()),
        };
        let raw = VersionedCheckpoint {
            id: "raw-id".to_string(),
            format_version: Some("2".to_string()),
        };
        manager.insert(stored, serde_json::to_vec(&raw).unwrap());
        let loaded = load_migrated(&manager, &version_manager, "cp-1")
            .await
            .unwrap();
        assert!(
            called.load(Ordering::SeqCst),
            "migration runs through the registered handler"
        );
        assert_eq!(
            loaded.id, "raw-id",
            "migration rewrites from raw bytes, never from the decoded row"
        );
    }

    #[tokio::test]
    async fn missing_migration_path_is_rejected() {
        let mut version_manager = VersionManager::new();
        version_manager.set_current_version("2").unwrap();
        let mut manager = StubManager::empty();
        let (checkpoint, raw) = StubManager::stored("cp-1", Some("1"));
        manager.insert(checkpoint, raw);
        let err = load_migrated(&manager, &version_manager, "cp-1")
            .await
            .unwrap_err();
        assert!(
            matches!(err, CheckpointError::VersionIncompatible { .. }),
            "migratable but handler-less versions fail: {err:?}"
        );
    }
}
