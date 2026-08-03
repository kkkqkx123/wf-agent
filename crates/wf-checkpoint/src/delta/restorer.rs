use crate::delta::{CheckpointLoader, DeltaRestorer, DiffCalculator};
use crate::error::CheckpointError;
use crate::serializer::CheckpointSerializer;
use std::sync::Arc;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::checkpoint::CheckpointType;
use wf_types::storage::CheckpointStorageMetadata;

pub struct GenericDeltaRestorer<SS, DS> {
    diff_calculator: Arc<dyn DiffCalculator<SS, DS>>,
}

impl<SS, DS> GenericDeltaRestorer<SS, DS> {
    pub fn new(diff_calculator: Arc<dyn DiffCalculator<SS, DS>>) -> Self {
        Self { diff_calculator }
    }
}

#[async_trait::async_trait]
impl<SS, DS> DeltaRestorer<SS, DS> for GenericDeltaRestorer<SS, DS>
where
    SS: serde::de::DeserializeOwned + Clone + Send + Sync,
    DS: serde::de::DeserializeOwned + Send + Sync,
{
    async fn restore_full_state(
        &self,
        target_checkpoint_id: &str,
        loader: &dyn CheckpointLoader,
    ) -> Result<SS, CheckpointError> {
        let chain = self.build_chain(target_checkpoint_id, loader).await?;

        let base = chain
            .first()
            .ok_or_else(|| CheckpointError::DeltaChainBroken {
                checkpoint_id: target_checkpoint_id.to_string(),
                missing_id: "base".to_string(),
            })?;

        let base_data = loader
            .load_checkpoint_data(&base.id)
            .await?
            .ok_or_else(|| CheckpointError::NotFound {
                id: base.id.clone(),
            })?;

        let base_envelope: BaseCheckpointCore<DS, SS> =
            CheckpointSerializer::auto_deserialize(&base_data)?;

        let mut full_state = base_envelope
            .snapshot
            .ok_or_else(|| CheckpointError::Corrupted {
                id: base.id.clone(),
                reason: "base checkpoint missing snapshot".to_string(),
            })?;

        for meta in chain.iter().skip(1) {
            let delta_data = loader
                .load_checkpoint_data(&meta.id)
                .await?
                .ok_or_else(|| CheckpointError::NotFound {
                    id: meta.id.clone(),
                })?;

            let envelope: BaseCheckpointCore<DS, SS> =
                CheckpointSerializer::auto_deserialize(&delta_data)?;

            let delta = envelope.delta.ok_or_else(|| CheckpointError::Corrupted {
                id: meta.id.clone(),
                reason: "delta checkpoint missing delta data".to_string(),
            })?;

            full_state = self
                .diff_calculator
                .apply_delta(&full_state, &delta)
                .await?;
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
            current_id = if is_base {
                None
            } else {
                meta.previous_checkpoint_id.clone()
            };
            chain.push(meta);

            if is_base {
                break;
            }
        }

        chain.reverse();
        Ok(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serializer::{CheckpointCodec, CheckpointSerializer};
    use std::collections::HashMap;
    use wf_types::checkpoint::CheckpointStatus;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestState {
        value: u32,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct TestDelta {
        increment: u32,
    }

    #[derive(Clone)]
    struct TestCalculator;

    #[async_trait::async_trait]
    impl DiffCalculator<TestState, TestDelta> for TestCalculator {
        async fn calculate_diff(
            &self,
            previous: &TestState,
            current: &TestState,
        ) -> Result<TestDelta, CheckpointError> {
            Ok(TestDelta {
                increment: current.value - previous.value,
            })
        }

        async fn apply_delta(
            &self,
            base: &TestState,
            delta: &TestDelta,
        ) -> Result<TestState, CheckpointError> {
            Ok(TestState {
                value: base.value + delta.increment,
            })
        }
    }

    struct FakeLoader {
        entries: HashMap<String, Vec<u8>>,
        metas: HashMap<String, CheckpointStorageMetadata>,
    }

    impl FakeLoader {
        fn new() -> Self {
            Self {
                entries: HashMap::new(),
                metas: HashMap::new(),
            }
        }

        fn add(
            &mut self,
            id: &str,
            previous_id: Option<&str>,
            checkpoint_type: CheckpointType,
            data: &BaseCheckpointCore<TestDelta, TestState>,
        ) {
            let meta = CheckpointStorageMetadata {
                id: id.to_string(),
                entity_type: "test".to_string(),
                entity_id: "entity-1".to_string(),
                checkpoint_type,
                timestamp: 0,
                status: CheckpointStatus::Completed,
                previous_checkpoint_id: previous_id.map(String::from),
                base_checkpoint_id: None,
                chain_root_id: None,
                chain_position: None,
                blob_size: None,
                tags: None,
                custom_fields: None,
            };
            let bytes = CheckpointSerializer::serialize(data, CheckpointCodec::Json).unwrap();
            self.entries.insert(id.to_string(), bytes);
            self.metas.insert(id.to_string(), meta);
        }
    }

    #[async_trait::async_trait]
    impl CheckpointLoader for FakeLoader {
        async fn load_checkpoint_data(&self, id: &str) -> Result<Option<Vec<u8>>, CheckpointError> {
            Ok(self.entries.get(id).cloned())
        }

        async fn load_metadata(
            &self,
            id: &str,
        ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
            Ok(self.metas.get(id).cloned())
        }
    }

    fn make_full(
        id: &str,
        state: TestState,
        previous: Option<&str>,
    ) -> BaseCheckpointCore<TestDelta, TestState> {
        BaseCheckpointCore {
            id: id.to_string(),
            r#type: Some(CheckpointType::Full),
            base_checkpoint_id: None,
            previous_checkpoint_id: previous.map(String::from),
            delta: None,
            snapshot: Some(state),
            timestamp: Some(0),
            metadata: None,
            format_version: None,
        }
    }

    fn make_delta(
        id: &str,
        base: &str,
        previous: Option<&str>,
        delta: TestDelta,
    ) -> BaseCheckpointCore<TestDelta, TestState> {
        BaseCheckpointCore {
            id: id.to_string(),
            r#type: Some(CheckpointType::Delta),
            base_checkpoint_id: Some(base.to_string()),
            previous_checkpoint_id: previous.map(String::from),
            delta: Some(delta),
            snapshot: None,
            timestamp: Some(0),
            metadata: None,
            format_version: None,
        }
    }

    #[tokio::test]
    async fn restore_replays_delta_chain() {
        let mut loader = FakeLoader::new();
        loader.add(
            "base",
            None,
            CheckpointType::Full,
            &make_full("base", TestState { value: 1 }, None),
        );
        loader.add(
            "d1",
            Some("base"),
            CheckpointType::Delta,
            &make_delta("d1", "base", Some("base"), TestDelta { increment: 1 }),
        );
        loader.add(
            "d2",
            Some("d1"),
            CheckpointType::Delta,
            &make_delta("d2", "base", Some("d1"), TestDelta { increment: 2 }),
        );

        let restorer = GenericDeltaRestorer::new(Arc::new(TestCalculator));
        let state = restorer.restore_full_state("d2", &loader).await.unwrap();
        assert_eq!(state.value, 4);
    }

    #[tokio::test]
    async fn restore_full_checkpoint_directly() {
        let mut loader = FakeLoader::new();
        loader.add(
            "base",
            None,
            CheckpointType::Full,
            &make_full("base", TestState { value: 7 }, None),
        );

        let restorer = GenericDeltaRestorer::new(Arc::new(TestCalculator));
        let state = restorer.restore_full_state("base", &loader).await.unwrap();
        assert_eq!(state.value, 7);
    }

    #[tokio::test]
    async fn restore_detects_chain_cycle() {
        let mut loader = FakeLoader::new();
        loader.add(
            "d1",
            Some("d1"),
            CheckpointType::Delta,
            &make_delta("d1", "base", Some("d1"), TestDelta { increment: 1 }),
        );

        let restorer = GenericDeltaRestorer::new(Arc::new(TestCalculator));
        let err = restorer
            .restore_full_state("d1", &loader)
            .await
            .unwrap_err();
        assert!(matches!(err, CheckpointError::Corrupted { .. }));
    }

    #[tokio::test]
    async fn restore_fails_when_chain_is_broken() {
        let mut loader = FakeLoader::new();
        loader.add(
            "d1",
            Some("missing"),
            CheckpointType::Delta,
            &make_delta("d1", "missing", Some("missing"), TestDelta { increment: 1 }),
        );

        let restorer = GenericDeltaRestorer::new(Arc::new(TestCalculator));
        let err = restorer
            .restore_full_state("d1", &loader)
            .await
            .unwrap_err();
        assert!(matches!(err, CheckpointError::NotFound { .. }));
    }
}
