use crate::client::LlmClient;
use crate::client_factory::ClientFactory;
use crate::error::{LlmError, LlmResult};
use wf_types::llm::{LlmRequest, LlmResult as LlmResponseType};

#[derive(Clone)]
pub struct LlmWrapper {
    factory: ClientFactory,
}

impl LlmWrapper {
    pub fn new() -> Self {
        Self {
            factory: ClientFactory::new(),
        }
    }

    pub fn with_factory(factory: ClientFactory) -> Self {
        Self { factory }
    }

    pub fn factory(&self) -> &ClientFactory {
        &self.factory
    }

    pub async fn generate(&self, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        let profile_id = request.profile_id.as_deref();
        let profile = self.factory.get_profile(profile_id)
            .ok_or_else(|| LlmError::ProfileNotFound(
                profile_id.unwrap_or("default").to_string()
            ))?;
        
        let client = self.factory.get_or_create(&profile);
        client.generate(request).await
    }

    pub async fn generate_stream(&self, request: &LlmRequest) -> LlmResult<Box<dyn crate::message_stream::MessageStream>> {
        let profile_id = request.profile_id.as_deref();
        let profile = self.factory.get_profile(profile_id)
            .ok_or_else(|| LlmError::ProfileNotFound(
                profile_id.unwrap_or("default").to_string()
            ))?;
        
        let client = self.factory.get_or_create(&profile);
        client.generate_stream(request).await
    }

    pub async fn count_tokens(&self, request: &LlmRequest) -> LlmResult<u32> {
        let profile_id = request.profile_id.as_deref();
        let profile = self.factory.get_profile(profile_id)
            .ok_or_else(|| LlmError::ProfileNotFound(
                profile_id.unwrap_or("default").to_string()
            ))?;
        
        let client = self.factory.get_or_create(&profile);
        client.count_tokens(request).await
    }
}

impl Default for LlmWrapper {
    fn default() -> Self {
        Self::new()
    }
}
