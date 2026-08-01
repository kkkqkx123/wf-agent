use crate::error::LlmResult;
use dashmap::DashMap;
use std::sync::Arc;
use wf_types::llm::LlmProfile;

#[derive(Clone)]
pub struct ProfileManager {
    profiles: Arc<DashMap<String, LlmProfile>>,
}

impl ProfileManager {
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(DashMap::new()),
        }
    }

    pub fn register(&self, profile: LlmProfile) -> LlmResult<()> {
        self.profiles.insert(profile.id.clone(), profile);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<LlmProfile> {
        self.profiles.get(id).map(|r| r.clone())
    }

    pub fn list(&self) -> Vec<LlmProfile> {
        self.profiles.iter().map(|r| r.clone()).collect()
    }

    pub fn remove(&self, id: &str) -> Option<LlmProfile> {
        self.profiles.remove(id).map(|(_, v)| v)
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}
