use crate::delta::{CheckpointLoader, DeltaRestorer};
use crate::error::CheckpointError;
use crate::serializer::CheckpointSerializer;
use wf_types::checkpoint::CheckpointType;
use wf_types::storage::CheckpointStorageMetadata;

pub struct GenericDeltaRestorer<SS, DS> {
    _ss: std::marker::PhantomData<SS>,
    _ds: std::marker::PhantomData<DS>,
}

impl<SS, DS> GenericDeltaRestorer<SS, DS> {
    pub fn new() -> Self {
        Self {
            _ss: std::marker::PhantomData,
            _ds: std::marker::PhantomData,
        }
    }
}

impl<SS, DS> Default for GenericDeltaRestorer<SS, DS> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl<SS, DS> DeltaRestorer<SS, DS> for GenericDeltaRestorer<SS, DS>
where
    SS: serde::Serialize + serde::de::DeserializeOwned + Clone + Send + Sync,
    DS: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    async fn restore_full_state(
        &self,
        target_checkpoint_id: &str,
        loader: &dyn CheckpointLoader,
    ) -> Result<SS, CheckpointError> {
        let mut chain = self.build_chain(target_checkpoint_id, loader).await?;

        chain.sort_by_key(|meta| meta.timestamp);

        let base = chain.first().ok_or_else(|| CheckpointError::DeltaChainBroken {
            checkpoint_id: target_checkpoint_id.to_string(),
            missing_id: "base".to_string(),
        })?;

        let base_data = loader
            .load_checkpoint_data(&base.id)
            .await?
            .ok_or_else(|| CheckpointError::NotFound {
                id: base.id.clone(),
            })?;

        let mut full_state: SS = CheckpointSerializer::auto_deserialize(&base_data)?;

        for meta in chain.iter().skip(1) {
            let delta_data = loader
                .load_checkpoint_data(&meta.id)
                .await?
                .ok_or_else(|| CheckpointError::NotFound {
                    id: meta.id.clone(),
                })?;

            let delta: DS = CheckpointSerializer::auto_deserialize(&delta_data)?;
            full_state = self.apply_delta_to_state(full_state, &delta).await?;
        }

        Ok(full_state)
    }
}

impl<SS, DS> GenericDeltaRestorer<SS, DS> {
    async fn build_chain(
        &self,
        target_id: &str,
        loader: &dyn CheckpointLoader,
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError> {
        let mut chain = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut current_id = Some(target_id.to_string());

        while let Some(id) = current_id {
            if !visited.insert(id.clone()) {
                return Err(CheckpointError::Corrupted {
                    id: id.clone(),
                    reason: "circular reference in delta chain".to_string(),
                });
            }

            let meta = loader
                .load_metadata(&id)
                .await?
                .ok_or_else(|| CheckpointError::NotFound { id: id.clone() })?;

            let is_base = meta.checkpoint_type == CheckpointType::Full;
            current_id = self.find_base_checkpoint_id(&meta, loader).await?;
            chain.push(meta);

            if is_base {
                break;
            }
        }

        Ok(chain)
    }

    async fn find_base_checkpoint_id(
        &self,
        meta: &CheckpointStorageMetadata,
        _loader: &dyn CheckpointLoader,
    ) -> Result<Option<String>, CheckpointError> {
        Ok(if meta.checkpoint_type == CheckpointType::Full {
            None
        } else {
            Some(meta.id.clone())
        })
    }

    async fn apply_delta_to_state(
        &self,
        base: SS,
        _delta: &DS,
    ) -> Result<SS, CheckpointError> {
        let _ = (base, _delta);
        Err(CheckpointError::Coordinator(
            "delta application not implemented for this type".to_string(),
        ))
    }
}
