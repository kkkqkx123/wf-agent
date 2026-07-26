use dashmap::DashMap;
use std::sync::Arc;

pub type RestoreFn = Arc<dyn Fn(&str, &[u8]) -> Result<(), crate::error::CheckpointError> + Send + Sync>;

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
        self.strategies.get(entity_type).map(|entry| entry.value().clone())
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

    #[test]
    fn register_and_get() {
        let registry = RestoreStrategyRegistry::new();
        let handler: RestoreFn = Arc::new(|_id, _data| Ok(()));
        registry.register("workflow", handler.clone());

        assert!(registry.is_registered("workflow"));
        assert!(!registry.is_registered("agent"));
        assert!(registry.get("workflow").is_some());
    }

    #[test]
    fn list_registered() {
        let registry = RestoreStrategyRegistry::new();
        registry.register("workflow", Arc::new(|_, _| Ok(())));
        registry.register("agent", Arc::new(|_, _| Ok(())));

        let mut list = registry.list();
        list.sort();
        assert_eq!(list, vec!["agent", "workflow"]);
    }

    #[test]
    fn execute_handler() {
        let registry = RestoreStrategyRegistry::new();
        registry.register("test", Arc::new(|id, _data| {
            if id == "ok" {
                Ok(())
            } else {
                Err(crate::error::CheckpointError::Internal("bad".to_string()))
            }
        }));

        let handler = registry.get("test").unwrap();
        assert!(handler("ok", b"").is_ok());
        assert!(handler("bad", b"").is_err());
    }
}
