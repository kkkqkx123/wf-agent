use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use llm_decision::{
    DecisionConfig, DecisionError, DecisionProvider, JevDecisionProvider, OpenAIDecisionProvider,
};
use wf_types::decision_profile::DecisionProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionProtocol {
    OpenAi,
    Jev,
}

impl DecisionProtocol {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "openai" => Some(Self::OpenAi),
            "jev" | "typesafe" | "systemone" => Some(Self::Jev),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Jev => "jev",
        }
    }
}

pub struct DecisionProviderConfig {
    pub protocol: DecisionProtocol,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub timeout_secs: u64,
    pub headers: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
}

impl DecisionProviderConfig {
    pub fn into_decision_config(self) -> DecisionConfig {
        let mut config = DecisionConfig::new(self.base_url, self.model);
        config.api_key = self.api_key;
        config.timeout_secs = self.timeout_secs;
        config.headers = self.headers;
        config.query_params = self.query_params;
        config
    }
}

pub enum AnyDecisionProvider {
    OpenAi(OpenAIDecisionProvider),
    Jev(JevDecisionProvider),
}

impl AnyDecisionProvider {
    pub fn from_config(config: DecisionProviderConfig) -> Result<Self, DecisionError> {
        let protocol = config.protocol;
        let inner = config.into_decision_config();
        match protocol {
            DecisionProtocol::OpenAi => Ok(Self::OpenAi(OpenAIDecisionProvider::new(inner)?)),
            DecisionProtocol::Jev => Ok(Self::Jev(JevDecisionProvider::new(inner)?)),
        }
    }

    pub fn protocol_name(&self) -> &'static str {
        match self {
            Self::OpenAi(provider) => provider.protocol_name(),
            Self::Jev(provider) => provider.protocol_name(),
        }
    }

    pub fn model_name(&self) -> &str {
        match self {
            Self::OpenAi(provider) => provider.model_name(),
            Self::Jev(provider) => provider.model_name(),
        }
    }
}

#[async_trait::async_trait]
impl DecisionProvider for AnyDecisionProvider {
    async fn create(
        &self,
        request: &llm_decision::DecisionRequest,
    ) -> Result<llm_decision::DecisionResponse, DecisionError> {
        match self {
            Self::OpenAi(provider) => provider.create(request).await,
            Self::Jev(provider) => provider.create(request).await,
        }
    }

    fn protocol_name(&self) -> &'static str {
        match self {
            Self::OpenAi(provider) => provider.protocol_name(),
            Self::Jev(provider) => provider.protocol_name(),
        }
    }

    fn model_name(&self) -> &str {
        match self {
            Self::OpenAi(provider) => provider.model_name(),
            Self::Jev(provider) => provider.model_name(),
        }
    }
}

#[derive(Debug, Default)]
pub struct DecisionProfileRegistry {
    profiles: DashMap<String, DecisionProfile>,
}

impl DecisionProfileRegistry {
    pub fn new() -> Self {
        Self {
            profiles: DashMap::new(),
        }
    }

    pub fn register(&self, profile: DecisionProfile) {
        self.profiles.insert(profile.id.clone(), profile);
    }

    pub fn get(&self, id: &str) -> Option<DecisionProfile> {
        self.profiles.get(id).map(|entry| entry.clone())
    }

    pub fn provider_config_for(
        &self,
        id: &str,
        api_key_override: Option<String>,
    ) -> Result<DecisionProviderConfig, DecisionError> {
        let profile = self
            .get(id)
            .ok_or_else(|| DecisionError::Config(format!("unknown decision profile '{id}'")))?;
        let protocol = DecisionProtocol::parse(&profile.protocol).ok_or_else(|| {
            DecisionError::Config(format!(
                "decision profile '{id}' has unknown protocol '{}'",
                profile.protocol
            ))
        })?;
        Ok(DecisionProviderConfig {
            protocol,
            base_url: profile.base_url,
            model: profile.model,
            api_key: api_key_override.or(profile.api_key),
            timeout_secs: profile.timeout_secs.unwrap_or(30),
            headers: profile.headers,
            query_params: profile.query_params,
        })
    }
}

pub type SharedDecisionProfileRegistry = Arc<DecisionProfileRegistry>;
