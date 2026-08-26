use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::LlmError;
use serde_json::Value;
use wf_types::message::Message;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NamedMessageContext {
    pub id: String,
    pub messages: Vec<Message>,
    pub created_at: i64,
    pub updated_at: i64,
    pub metadata: Option<HashMap<String, Value>>,
}

pub struct MessageContextRegistry {
    contexts: Mutex<HashMap<String, NamedMessageContext>>,
}

impl MessageContextRegistry {
    pub fn new() -> Self {
        Self {
            contexts: Mutex::new(HashMap::new()),
        }
    }

    pub fn set(&self, name: &str, messages: Vec<Message>) {
        let now = wf_common::now();
        wf_common::lock::lock_ok(self.contexts.lock()).insert(
            name.to_string(),
            NamedMessageContext {
                id: name.to_string(),
                messages,
                created_at: now,
                updated_at: now,
                metadata: None,
            },
        );
    }

    pub fn update(&self, name: &str, messages: Vec<Message>) -> Result<(), LlmError> {
        let mut contexts = wf_common::lock::lock_ok(self.contexts.lock());
        match contexts.get_mut(name) {
            Some(ctx) => {
                ctx.messages = messages;
                ctx.updated_at = wf_common::now();
                Ok(())
            }
            None => Err(LlmError::ConfigError(format!(
                "context '{}' does not exist, cannot update",
                name
            ))),
        }
    }

    pub fn get(&self, name: &str) -> Option<Vec<Message>> {
        self.contexts
            .lock()
            .unwrap()
            .get(name)
            .map(|ctx| ctx.messages.clone())
    }

    pub fn get_named(&self, name: &str) -> Option<NamedMessageContext> {
        wf_common::lock::lock_ok(self.contexts.lock())
            .get(name)
            .cloned()
    }

    pub fn append(&self, name: &str, message: Message) {
        let mut contexts = wf_common::lock::lock_ok(self.contexts.lock());
        let now = wf_common::now();
        let ctx = contexts
            .entry(name.to_string())
            .or_insert_with(|| NamedMessageContext {
                id: name.to_string(),
                messages: Vec::new(),
                created_at: now,
                updated_at: now,
                metadata: None,
            });
        ctx.messages.push(message);
        ctx.updated_at = now;
    }

    pub fn remove(&self, name: &str) -> bool {
        wf_common::lock::lock_ok(self.contexts.lock())
            .remove(name)
            .is_some()
    }

    pub fn list(&self) -> Vec<String> {
        wf_common::lock::lock_ok(self.contexts.lock())
            .keys()
            .cloned()
            .collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        wf_common::lock::lock_ok(self.contexts.lock()).contains_key(name)
    }

    pub fn clear(&self) {
        wf_common::lock::lock_ok(self.contexts.lock()).clear();
    }

    pub fn get_all(&self) -> Vec<NamedMessageContext> {
        wf_common::lock::lock_ok(self.contexts.lock())
            .values()
            .cloned()
            .collect()
    }

    pub fn get_or_create_context(&self, name: &str) -> NamedMessageContext {
        let mut contexts = wf_common::lock::lock_ok(self.contexts.lock());
        if let Some(ctx) = contexts.get(name) {
            ctx.clone()
        } else {
            let now = wf_common::now();
            let ctx = NamedMessageContext {
                id: name.to_string(),
                messages: Vec::new(),
                created_at: now,
                updated_at: now,
                metadata: None,
            };
            contexts.insert(name.to_string(), ctx.clone());
            ctx
        }
    }

    pub fn set_metadata(
        &self,
        name: &str,
        metadata: HashMap<String, Value>,
    ) -> Result<(), LlmError> {
        let mut contexts = wf_common::lock::lock_ok(self.contexts.lock());
        match contexts.get_mut(name) {
            Some(ctx) => {
                ctx.metadata = Some(metadata);
                ctx.updated_at = wf_common::now();
                Ok(())
            }
            None => Err(LlmError::ConfigError(format!(
                "context '{}' does not exist, cannot set metadata",
                name
            ))),
        }
    }

    pub fn initialize_execution_context(&self) -> NamedMessageContext {
        self.get_or_create_context("current")
    }
}

impl Default for MessageContextRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_message(text: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn test_set_and_get() {
        let registry = MessageContextRegistry::new();
        let msgs = vec![make_message("hello")];
        registry.set("ctx1", msgs.clone());
        assert_eq!(registry.get("ctx1").unwrap().len(), 1);
    }

    #[test]
    fn test_get_missing() {
        let registry = MessageContextRegistry::new();
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn test_append() {
        let registry = MessageContextRegistry::new();
        registry.append("ctx1", make_message("msg1"));
        registry.append("ctx1", make_message("msg2"));
        assert_eq!(registry.get("ctx1").unwrap().len(), 2);
    }

    #[test]
    fn test_remove() {
        let registry = MessageContextRegistry::new();
        registry.set("ctx1", vec![make_message("hello")]);
        assert!(registry.remove("ctx1"));
        assert!(!registry.remove("ctx1"));
        assert!(registry.get("ctx1").is_none());
    }

    #[test]
    fn test_list() {
        let registry = MessageContextRegistry::new();
        registry.set("ctx_a", vec![]);
        registry.set("ctx_b", vec![]);
        registry.set("ctx_c", vec![]);
        let mut names = registry.list();
        names.sort();
        assert_eq!(names, vec!["ctx_a", "ctx_b", "ctx_c"]);
    }

    #[test]
    fn test_contains() {
        let registry = MessageContextRegistry::new();
        assert!(!registry.contains("ctx1"));
        registry.set("ctx1", vec![]);
        assert!(registry.contains("ctx1"));
    }

    #[test]
    fn test_clear() {
        let registry = MessageContextRegistry::new();
        registry.set("ctx1", vec![make_message("hello")]);
        registry.set("ctx2", vec![make_message("world")]);
        registry.clear();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_update() {
        let registry = MessageContextRegistry::new();
        registry.set("ctx1", vec![make_message("old")]);
        registry.update("ctx1", vec![make_message("new")]).unwrap();
        let msgs = registry.get("ctx1").unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs[0].content,
            wf_types::message::MessageContentValue::Text("new".to_string())
        );
    }

    #[test]
    fn test_update_missing_errors() {
        let registry = MessageContextRegistry::new();
        let result = registry.update("nonexistent", vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_named() {
        let registry = MessageContextRegistry::new();
        registry.set("ctx1", vec![make_message("hello")]);
        let named = registry.get_named("ctx1").unwrap();
        assert_eq!(named.id, "ctx1");
        assert_eq!(named.messages.len(), 1);
        assert!(named.created_at > 0);
        assert!(named.updated_at >= named.created_at);
    }

    #[test]
    fn test_get_all() {
        let registry = MessageContextRegistry::new();
        registry.set("ctx1", vec![make_message("a")]);
        registry.set("ctx2", vec![make_message("b")]);
        let all = registry.get_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_timestamps_update_on_append() {
        let registry = MessageContextRegistry::new();
        registry.set("ctx1", vec![make_message("msg1")]);
        let before = registry.get_named("ctx1").unwrap();
        let created = before.created_at;

        registry.append("ctx1", make_message("msg2"));
        let after = registry.get_named("ctx1").unwrap();
        assert_eq!(after.created_at, created);
        assert!(after.updated_at >= before.updated_at);
    }

    #[test]
    fn test_get_or_create_existing() {
        let registry = MessageContextRegistry::new();
        registry.set("ctx1", vec![make_message("hello")]);
        let ctx = registry.get_or_create_context("ctx1");
        assert_eq!(ctx.messages.len(), 1);
    }

    #[test]
    fn test_get_or_create_new() {
        let registry = MessageContextRegistry::new();
        let ctx = registry.get_or_create_context("new_ctx");
        assert_eq!(ctx.id, "new_ctx");
        assert!(ctx.messages.is_empty());
        assert!(ctx.created_at > 0);
    }

    #[test]
    fn test_set_metadata() {
        let registry = MessageContextRegistry::new();
        registry.set("ctx1", vec![]);
        let mut meta = HashMap::new();
        meta.insert("source".to_string(), Value::String("node-a".to_string()));
        registry.set_metadata("ctx1", meta).unwrap();

        let named = registry.get_named("ctx1").unwrap();
        assert!(named.metadata.is_some());
        let meta = named.metadata.unwrap();
        assert_eq!(
            meta.get("source").unwrap(),
            &Value::String("node-a".to_string())
        );
    }

    #[test]
    fn test_set_metadata_missing_errors() {
        let registry = MessageContextRegistry::new();
        let result = registry.set_metadata("nonexistent", HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_initialize_execution_context() {
        let registry = MessageContextRegistry::new();
        let ctx = registry.initialize_execution_context();
        assert_eq!(ctx.id, "current");
        assert!(ctx.messages.is_empty());
        assert!(registry.contains("current"));
    }
}
