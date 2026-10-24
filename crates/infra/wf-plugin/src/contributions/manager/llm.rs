use super::*;

impl ContributionManager {
    pub fn get_llm_codec(&self, name: &str) -> Option<Arc<dyn PluginLlmCodec>> {
        self.llm_registry.get(name)
    }

    /// Auth implementation contributed by a plugin, keyed by the name a
    /// provider definition's `auth.provider` references.
    pub fn get_llm_auth(&self, name: &str) -> Option<Arc<dyn PluginLlmAuth>> {
        self.llm_auth_registry.get(name)
    }

    /// All plugin-contributed auth implementations as `(name, owner)` pairs.
    pub fn all_llm_auths(&self) -> Vec<(String, String)> {
        self.llm_auth_registry.all()
    }

    /// Declarative provider definition contributed by a plugin.
    pub fn get_llm_provider_definition(
        &self,
        id: &str,
    ) -> Option<Arc<wf_types::llm::LlmProviderDefinition>> {
        self.llm_provider_definitions.get(id)
    }

    pub fn all_llm_providers(&self) -> Vec<(String, String)> {
        self.llm_registry.all()
    }

    /// All plugin-contributed provider definitions as `(id, owner)` pairs.
    pub fn all_llm_provider_definitions(&self) -> Vec<(String, String)> {
        self.llm_provider_definitions.all()
    }
}
