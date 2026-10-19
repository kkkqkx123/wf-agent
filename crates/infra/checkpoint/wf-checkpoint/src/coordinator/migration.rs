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

    let version = checkpoint.format_version().ok_or_else(|| CheckpointError::Validation {
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
