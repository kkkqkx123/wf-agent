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
        self.contexts.lock().unwrap().insert(name.to_string(), messages);
    }

    pub fn get(&self, name: &str) -> Option<Vec<Message>> {
        self.contexts.lock().unwrap().get(name).cloned()
    }

    pub fn append(&self, name: &str, message: Message) {
        self.contexts.lock().unwrap()
            .entry(name.to_string())
            .or_default()
            .push(message);
    }
}
