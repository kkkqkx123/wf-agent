use crate::error::LlmResult;
use dashmap::DashMap;
use std::sync::Arc;
use wf_types::llm::LlmProfile;

#[derive(Clone)]
pub struct ProfileManager {
    profiles: Arc<DashMap<String, LlmProfile>>,
    default_profile_id: Arc<tokio::sync::RwLock<Option<String>>>,
}

impl ProfileManager {
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(DashMap::new()),
            default_profile_id: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub fn register(&self, profile: LlmProfile) -> LlmResult<()> {
        self.profiles.insert(profile.id.clone(), profile);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<LlmProfile> {
        self.profiles.get(id).map(|r| r.clone())
    }

    pub async fn get_default(&self) -> Option<LlmProfile> {
        let guard = self.default_profile_id.read().await;
        guard.as_ref().and_then(|id| self.get(id))
    }

    pub async fn set_default(&self, profile_id: String) {
        let mut guard = self.default_profile_id.write().await;
        *guard = Some(profile_id);
    }

    pub fn list(&self) -> Vec<LlmProfile> {
        self.profiles.iter().map(|r| r.clone()).collect()
    }

    pub fn remove(&self, id: &str) -> Option<LlmProfile> {
        self.profiles.remove(id).map(|(_, v)| v)
    }

    pub fn get_or_default(&self, profile_id: Option<&str>) -> Option<LlmProfile> {
        if let Some(id) = profile_id {
            self.get(id)
        } else {
            None
        }
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}
