use crate::client::LlmClientImpl;
use crate::formatters::create_formatter;
use dashmap::DashMap;
use reqwest::Client as ReqwestClient;
use std::sync::Arc;
use wf_types::llm::LlmProfile;

#[derive(Clone)]
pub struct ClientFactory {
    clients: Arc<DashMap<String, Arc<LlmClientImpl>>>,
    profile_manager: crate::profile_manager::ProfileManager,
    #[cfg(feature = "mock")]
    mock_clients: Arc<DashMap<String, Arc<crate::mock::MockLlmClient>>>,
}

impl ClientFactory {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            profile_manager: crate::profile_manager::ProfileManager::new(),
            #[cfg(feature = "mock")]
            mock_clients: Arc::new(DashMap::new()),
        }
    }

    /// Register a mock client under an arbitrary id (a real profile id can be
    /// reused, or a plain "mock" id). Mock hits take priority over profiles.
    #[cfg(feature = "mock")]
    pub fn register_mock(&self, id: impl Into<String>, client: Arc<crate::mock::MockLlmClient>) {
        self.mock_clients.insert(id.into(), client);
    }

    #[cfg(feature = "mock")]
    pub fn mock_client(&self, id: &str) -> Option<Arc<crate::mock::MockLlmClient>> {
        self.mock_clients.get(id).map(|c| c.clone())
    }

    pub fn with_profiles(profiles: Vec<LlmProfile>) -> Self {
        let factory = Self::new();
        for profile in profiles {
            let _ = factory.profile_manager.register(profile.clone());
        }
        factory
    }

    pub fn get_or_create(&self, profile: &LlmProfile) -> Arc<LlmClientImpl> {
        let key = &profile.id;

        if let Some(client) = self.clients.get(key) {
            return client.clone();
        }

        let formatter = create_formatter(&profile.provider);
        let client = ReqwestClient::builder()
            .timeout(std::time::Duration::from_secs(
                profile.timeout.unwrap_or(60),
            ))
            .build()
            .unwrap_or_default();

        let client_impl = Arc::new(LlmClientImpl::new(client, formatter, profile.clone()));
        self.clients.insert(key.clone(), client_impl.clone());
        client_impl
    }

    pub fn get_profile(&self, profile_id: Option<&str>) -> Option<LlmProfile> {
        if let Some(id) = profile_id {
            self.profile_manager.get(id)
        } else {
            None
        }
    }

    pub fn register_profile(&self, profile: LlmProfile) -> crate::error::LlmResult<()> {
        self.profile_manager.register(profile)
    }
}

impl Default for ClientFactory {
    fn default() -> Self {
        Self::new()
    }
}
