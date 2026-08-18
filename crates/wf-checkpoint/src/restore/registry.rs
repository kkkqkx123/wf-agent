use dashmap::DashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::CheckpointError;

/// Async restore function for an execution type. Receives the checkpoint id
/// and the raw checkpoint bytes (owned, so the returned future is `'static`);
/// returns the restored entity as JSON.
pub type RestoreFn = Arc<
    dyn Fn(
            String,
            Vec<u8>,
        )
            -> Pin<Box<dyn Future<Output = Result<serde_json::Value, CheckpointError>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct RestoreStrategyRegistry {
    strategies: DashMap<String, RestoreFn>,
}

impl RestoreStrategyRegistry {
    pub fn new() -> Self {
        Self {
            strategies: DashMap::new(),
        }
    }

    pub fn register(&self, entity_type: impl Into<String>, handler: RestoreFn) {
        self.strategies.insert(entity_type.into(), handler);
    }

    pub fn get(&self, entity_type: &str) -> Option<RestoreFn> {
        self.strategies
            .get(entity_type)
            .map(|entry| entry.value().clone())
    }

    /// Run the registered restore function for `entity_type` against the raw
    /// checkpoint bytes. Returns the restored entity as JSON.
    pub async fn restore(
        &self,
        entity_type: &str,
        checkpoint_id: &str,
        data: &[u8],
    ) -> Result<serde_json::Value, CheckpointError> {
        let handler = self.get(entity_type).ok_or_else(|| {
            CheckpointError::Coordinator(format!(
                "no restore strategy registered for entity type '{}'",
                entity_type
            ))
        })?;
        handler(checkpoint_id.to_string(), data.to_vec()).await
    }

    pub fn is_registered(&self, entity_type: &str) -> bool {
        self.strategies.contains_key(entity_type)
    }

    pub fn list(&self) -> Vec<String> {
        self.strategies
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }
}

impl Default for RestoreStrategyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_handler() -> RestoreFn {
        Arc::new(|_id, data| {
            Box::pin(async move {
                let value: serde_json::Value = serde_json::from_slice(&data).map_err(|e| {
                    CheckpointError::Serialization(format!("failed to parse entity: {}", e))
                })?;
                Ok(value)
            })
        })
    }

    #[tokio::test]
    async fn register_and_get() {
        let registry = RestoreStrategyRegistry::new();
        registry.register("workflow", identity_handler());

        assert!(registry.is_registered("workflow"));
        assert!(!registry.is_registered("agent"));
        assert!(registry.get("workflow").is_some());
    }

    #[tokio::test]
    async fn restore_returns_entity() {
        let registry = RestoreStrategyRegistry::new();
        registry.register("agent", identity_handler());

        let entity = registry
            .restore("agent", "cp-1", b"{\"agent_loop_id\":\"loop-1\"}")
            .await
            .unwrap();
        assert_eq!(entity["agent_loop_id"], serde_json::json!("loop-1"));
    }

    #[tokio::test]
    async fn restore_missing_strategy_fails() {
        let registry = RestoreStrategyRegistry::new();
        assert!(registry.restore("unknown", "cp-1", b"").await.is_err());
    }

    #[tokio::test]
    async fn handler_failure_propagates() {
        let registry = RestoreStrategyRegistry::new();
        registry.register(
            "test",
            Arc::new(|_id, _data| {
                Box::pin(async { Err(CheckpointError::Internal("bad data".to_string())) })
            }),
        );

        assert!(registry.restore("test", "cp-1", b"{}").await.is_err());
    }

    #[test]
    fn list_registered() {
        let registry = RestoreStrategyRegistry::new();
        registry.register("workflow", identity_handler());
        registry.register("agent", identity_handler());

        let mut list = registry.list();
        list.sort();
        assert_eq!(list, vec!["agent", "workflow"]);
    }
}
