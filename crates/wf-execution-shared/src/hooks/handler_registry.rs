use std::collections::HashMap;
use std::sync::RwLock;

use crate::hooks::executor::HookHandler;

pub struct HookHandlerRegistry {
    handlers: RwLock<HashMap<String, Vec<HookHandler>>>,
}

impl HookHandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, hook_type: impl Into<String>, handler: HookHandler) {
        let mut handlers = self.handlers.write().unwrap();
        handlers.entry(hook_type.into()).or_default().push(handler);
    }

    pub fn get_handlers(&self, hook_type: &str) -> Vec<HookHandler> {
        self.handlers
            .read()
            .unwrap()
            .get(hook_type)
            .cloned()
            .unwrap_or_default()
    }

    pub fn remove(&self, hook_type: &str) -> bool {
        self.handlers
            .write()
            .unwrap()
            .remove(hook_type)
            .is_some()
    }

    pub fn list_types(&self) -> Vec<String> {
        self.handlers.read().unwrap().keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.read().unwrap().is_empty()
    }

    pub fn clear(&self) {
        self.handlers.write().unwrap().clear();
    }
}

impl Default for HookHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::hooks::types::{BaseHookContext, HookExecutionResult};
    use futures::future::BoxFuture;

    fn make_handler(id: &str) -> HookHandler {
        let id = id.to_string();
        Arc::new(move |ctx: BaseHookContext| -> BoxFuture<'static, HookExecutionResult> {
            let id = id.clone();
            let exec_id = ctx.execution_id.clone();
            Box::pin(async move {
                HookExecutionResult {
                    hook_id: exec_id,
                    success: true,
                    error: Some(id),
                }
            })
        })
    }

    #[test]
    fn test_register_and_get() {
        let registry = HookHandlerRegistry::new();
        registry.register("before_iteration", make_handler("h1"));
        registry.register("before_iteration", make_handler("h2"));

        let handlers = registry.get_handlers("before_iteration");
        assert_eq!(handlers.len(), 2);
    }

    #[test]
    fn test_get_handlers_empty() {
        let registry = HookHandlerRegistry::new();
        let handlers = registry.get_handlers("nonexistent");
        assert!(handlers.is_empty());
    }

    #[test]
    fn test_remove() {
        let registry = HookHandlerRegistry::new();
        registry.register("test", make_handler("h1"));
        assert!(registry.remove("test"));
        assert!(!registry.remove("test"));
    }

    #[test]
    fn test_list_types() {
        let registry = HookHandlerRegistry::new();
        registry.register("type_a", make_handler("h1"));
        registry.register("type_b", make_handler("h2"));
        let mut types = registry.list_types();
        types.sort();
        assert_eq!(types, vec!["type_a", "type_b"]);
    }

    #[test]
    fn test_clear() {
        let registry = HookHandlerRegistry::new();
        registry.register("type_a", make_handler("h1"));
        registry.register("type_b", make_handler("h2"));
        registry.clear();
        assert!(registry.is_empty());
    }
}
