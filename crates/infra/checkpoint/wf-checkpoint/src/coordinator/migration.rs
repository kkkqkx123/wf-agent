use checkpoint_base::delta::CheckpointLoader;
use checkpoint_base::error::CheckpointError;
use checkpoint_state::state::CheckpointStateManager;

/// Load a checkpoint blob. No version migration is performed; the development
/// format is locked to a single version.
pub async fn load_migrated<M>(
    manager: &M,
    checkpoint_id: &str,
) -> Result<M::Checkpoint, CheckpointError>
where
    M: CheckpointStateManager + CheckpointLoader,
    M::Checkpoint: serde::de::DeserializeOwned,
{
    manager
        .load(checkpoint_id)
        .await?
        .ok_or_else(|| CheckpointError::NotFound {
            id: checkpoint_id.to_string(),
        })
}
