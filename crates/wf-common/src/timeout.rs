use std::collections::HashMap;
use std::time::Duration;

pub struct TimeoutManager {
    timeouts: HashMap<String, Duration>,
}

impl Default for TimeoutManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeoutManager {
    pub fn new() -> Self {
        Self {
            timeouts: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: impl Into<String>, duration: Duration) {
        self.timeouts.insert(name.into(), duration);
    }

    pub fn get(&self, name: &str) -> Option<Duration> {
        self.timeouts.get(name).copied()
    }

    pub fn cancel(&mut self, name: &str) {
        self.timeouts.remove(name);
    }

    pub fn clear(&mut self) {
        self.timeouts.clear();
    }
}
