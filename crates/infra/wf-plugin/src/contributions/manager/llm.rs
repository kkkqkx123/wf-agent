use super::*;

impl ContributionManager {
    pub fn get_llm_codec(&self, name: &str) -> Option<Arc<dyn PluginLlmCodec>> {
        self.llm_registry.get(name)
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
