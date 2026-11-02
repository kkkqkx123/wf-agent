use crate::coordinator::base::{CheckpointBlob, CheckpointId};
use crate::coordinator::events::{publish_persist_failed, publish_persisted};
use checkpoint_base::error::CheckpointError;
use checkpoint_base::error_handling::CheckpointErrorHandler;
use checkpoint_file::event::CheckpointEventBus;
use checkpoint_state::state::CheckpointStateManager;

/// Persist one checkpoint row, publishing the outcome on the shared bus.
/// A failed write is routed through the checkpoint error handler: the default
/// handler surfaces the failure to the caller so a failed write never
/// masquerades as a saved checkpoint. Only an explicitly lenient handler
/// lets the execution continue without one.
pub async fn persist_checkpoint<M>(
    manager: &M,
    checkpoint: &M::Checkpoint,
    entity_type: &str,
    entity_id: &str,
    bus: Option<&CheckpointEventBus>,
    error_handler: &CheckpointErrorHandler,
) -> Result<(), CheckpointError>
where
    M: CheckpointStateManager,
    M::Checkpoint: CheckpointId + CheckpointBlob,
{
    let checkpoint_id = checkpoint.checkpoint_id().to_string();
    if let Err(err) = manager.save(checkpoint, entity_type, entity_id).await {
        publish_persist_failed(bus, Some(checkpoint_id.clone()), entity_id, &err);
        let context = error_handler.context("create", Some(checkpoint_id), None);
        let outcome = error_handler.decide(&context, &err);
        if outcome.should_rethrow {
            return Err(err);
        }
        return Ok(());
    }

    let description = checkpoint.blob_description();
    publish_persisted(bus, &checkpoint_id, entity_id, description);
    Ok(())
}

/// Delete one checkpoint row, publishing the deletion when the row existed.
pub async fn delete_checkpoint<M>(
    manager: &M,
    checkpoint_id: &str,
    bus: Option<&CheckpointEventBus>,
) -> Result<bool, CheckpointError>
where
    M: CheckpointStateManager,
{
    let deleted = manager.delete(checkpoint_id).await?;
    if deleted {
        if let Some(bus) = bus {
            bus.publish(CheckpointEventBus::deleted_with(
                checkpoint_id.to_string(),
                Some("delete".to_string()),
            ));
        }
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct TestCheckpoint {
        id: String,
        description: Option<String>,
        metadata: Option<HashMap<String, serde_json::Value>>,
    }

    impl CheckpointId for TestCheckpoint {
        fn checkpoint_id(&self) -> &str {
            &self.id
        }
    }

    impl CheckpointBlob for TestCheckpoint {
        fn blob_description(&self) -> Option<&str> {
            self.description.as_deref()
        }

        fn blob_metadata_mut(&mut self) -> &mut Option<HashMap<String, serde_json::Value>> {
            &mut self.metadata
        }
    }

    struct StubManager {
        fail_save: bool,
        deleted: bool,
    }

    impl CheckpointStateManager for StubManager {
        type Checkpoint = TestCheckpoint;

        async fn save(
            &self,
            _checkpoint: &TestCheckpoint,
            _entity_type: &str,
            _entity_id: &str,
        ) -> Result<(), CheckpointError> {
            if self.fail_save {
                Err(CheckpointError::Coordinator(
                    "injected save failure".to_string(),
                ))
            } else {
                Ok(())
            }
        }

        async fn load(&self, _id: &str) -> Result<Option<TestCheckpoint>, CheckpointError> {
            Ok(None)
        }

        async fn load_batch(
            &self,
            ids: &[String],
        ) -> Result<Vec<Option<TestCheckpoint>>, CheckpointError> {
            Ok(vec![None; ids.len()])
        }

        async fn delete(&self, _id: &str) -> Result<bool, CheckpointError> {
            Ok(self.deleted)
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

    fn checkpoint() -> TestCheckpoint {
        TestCheckpoint {
            id: "cp-1".to_string(),
            description: Some("note".to_string()),
            metadata: None,
        }
    }

    #[tokio::test]
    async fn strict_handler_rethrows_save_failure_and_publishes_failed() {
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();
        let manager = StubManager {
            fail_save: true,
            deleted: false,
        };
        let err = persist_checkpoint(
            &manager,
            &checkpoint(),
            "agent_loop",
            "loop-1",
            Some(&bus),
            &CheckpointErrorHandler::default(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, CheckpointError::Coordinator(_)),
            "strict default surfaces the write failure: {err:?}"
        );
        let event = rx.try_recv().expect("persist failure is published");
        assert!(
            matches!(
                event,
                checkpoint_file::event::CheckpointEvent::Failed { .. }
            ),
            "failed writes stay queryable as Failed events"
        );
    }

    #[tokio::test]
    async fn lenient_handler_swallows_save_failure_but_still_publishes() {
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();
        let manager = StubManager {
            fail_save: true,
            deleted: false,
        };
        persist_checkpoint(
            &manager,
            &checkpoint(),
            "agent_loop",
            "loop-1",
            Some(&bus),
            &CheckpointErrorHandler::new(false),
        )
        .await
        .expect("explicitly lenient handler swallows the failure");
        assert!(
            matches!(
                rx.try_recv().expect("failure still published"),
                checkpoint_file::event::CheckpointEvent::Failed { .. }
            ),
            "swallowed failures are still observable on the bus"
        );
    }

    #[tokio::test]
    async fn delete_missing_row_reports_false_without_event() {
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();
        let manager = StubManager {
            fail_save: false,
            deleted: false,
        };
        let deleted = delete_checkpoint(&manager, "cp-x", Some(&bus))
            .await
            .unwrap();
        assert!(!deleted);
        assert!(
            rx.try_recv().is_err(),
            "a row that never existed publishes nothing"
        );
    }

    #[tokio::test]
    async fn delete_existing_row_publishes_deleted() {
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();
        let manager = StubManager {
            fail_save: false,
            deleted: true,
        };
        let deleted = delete_checkpoint(&manager, "cp-1", Some(&bus))
            .await
            .unwrap();
        assert!(deleted);
        let event = rx.try_recv().expect("deletion is published");
        match event {
            checkpoint_file::event::CheckpointEvent::Deleted { data, .. } => {
                assert_eq!(data.checkpoint_id.as_deref(), Some("cp-1"));
            }
            other => panic!("expected Deleted, got {other:?}"),
        }
    }
}
