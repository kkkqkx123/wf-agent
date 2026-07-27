use std::collections::HashMap;
use std::sync::Mutex;

use wf_types::message::Message;

pub struct MessageContextRegistry {
    contexts: Mutex<HashMap<String, Vec<Message>>>,
}

impl MessageContextRegistry {
    pub fn new() -> Self {
        Self {
            contexts: Mutex::new(HashMap::new()),
        }
    }

    pub fn set(&self, name: &str, messages: Vec<Message>) {
        self.contexts
            .lock()
            .unwrap()
            .insert(name.to_string(), messages);
    }

    pub fn get(&self, name: &str) -> Option<Vec<Message>> {
        self.contexts.lock().unwrap().get(name).cloned()
    }

    pub fn append(&self, name: &str, message: Message) {
        self.contexts
            .lock()
            .unwrap()
            .entry(name.to_string())
            .or_default()
            .push(message);
    }

    pub fn remove(&self, name: &str) -> bool {
        self.contexts.lock().unwrap().remove(name).is_some()
    }

    pub fn list(&self) -> Vec<String> {
        self.contexts.lock().unwrap().keys().cloned().collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.contexts.lock().unwrap().contains_key(name)
    }

    pub fn clear(&self) {
        self.contexts.lock().unwrap().clear();
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
}
