use super::*;

impl ContributionManager {
    pub fn get_node_handler(&self, type_name: &str) -> Option<Arc<dyn PluginNodeHandler>> {
        self.node_type_registry.get(type_name)
    }

    pub fn get_tool_executor(&self, type_name: &str) -> Option<Arc<dyn PluginToolExecutor>> {
        self.tool_type_registry.get(type_name)
    }

    pub fn all_node_types(&self) -> Vec<(String, String)> {
        self.node_type_registry.all()
    }

    pub fn all_tool_types(&self) -> Vec<(String, String)> {
        self.tool_type_registry.all()
    }

    pub fn get_compaction_advisor(&self, name: &str) -> Option<Arc<dyn PluginCompactionAdvisor>> {
        self.compaction_advisor_registry.get(name)
    }

    pub fn all_compaction_advisors(&self) -> Vec<(String, String)> {
        self.compaction_advisor_registry.all()
    }

    pub fn get_text_analyzer(&self, name: &str) -> Option<Arc<dyn PluginTextAnalyzer>> {
        self.text_analyzer_registry.get(name)
    }

    pub fn all_text_analyzers(&self) -> Vec<(String, String)> {
        self.text_analyzer_registry.all()
    }

    pub fn get_retrieval_advisor(&self, name: &str) -> Option<Arc<dyn PluginRetrievalAdvisor>> {
        self.retrieval_advisor_registry.get(name)
    }

    pub fn all_retrieval_advisors(&self) -> Vec<(String, String)> {
        self.retrieval_advisor_registry.all()
    }

    pub fn get_vector_encoding(&self, name: &str) -> Option<Arc<dyn PluginVectorEncoding>> {
        self.vector_encoding_registry.get(name)
    }

    pub fn all_vector_encodings(&self) -> Vec<(String, String)> {
        self.vector_encoding_registry.all()
    }

    pub fn get_snapshot_policy(&self, name: &str) -> Option<Arc<Value>> {
        self.snapshot_policy_registry.get(name)
    }

    pub fn all_snapshot_policies(&self) -> Vec<(String, String)> {
        self.snapshot_policy_registry.all()
    }

    /// Owned CLI command snapshot for hosts that render without holding
    /// the manager (the CLI shells fetch this projection over the API).
    pub fn cli_command_snapshot(&self) -> Vec<PluginCliCommand> {
        self.cli_command_registry
            .all_with_values()
            .into_iter()
            .map(|(_, _, command)| (*command).clone())
            .collect()
    }

    pub fn get_cli_command(&self, id: &str) -> Option<Arc<PluginCliCommand>> {
        self.cli_command_registry.get(id)
    }

    pub fn all_cli_commands(&self) -> Vec<(String, String)> {
        self.cli_command_registry.all()
    }

    pub fn completion_provider_snapshot(&self) -> Vec<PluginCompletionSource> {
        self.completion_provider_registry
            .all_with_values()
            .into_iter()
            .map(|(_, _, source)| (*source).clone())
            .collect()
    }

    pub fn get_completion_provider(&self, id: &str) -> Option<Arc<PluginCompletionSource>> {
        self.completion_provider_registry.get(id)
    }

    pub fn all_completion_providers(&self) -> Vec<(String, String)> {
        self.completion_provider_registry.all()
    }

    pub fn setting_field_snapshot(&self) -> Vec<PluginSettingField> {
        self.setting_field_registry
            .all_with_values()
            .into_iter()
            .map(|(_, _, field)| (*field).clone())
            .collect()
    }

    pub fn get_setting_field(&self, key: &str) -> Option<Arc<PluginSettingField>> {
        self.setting_field_registry.get(key)
    }

    pub fn all_setting_fields(&self) -> Vec<(String, String)> {
        self.setting_field_registry.all()
    }

    /// Owned tool summary snapshot for hosts that render without
    /// holding the manager (served through the CLI snapshot API).
    pub fn tool_summary_snapshot(&self) -> Vec<PluginToolSummary> {
        self.tool_summary_registry
            .all_with_values()
            .into_iter()
            .map(|(_, _, summary)| (*summary).clone())
            .collect()
    }

    pub fn get_tool_summary(&self, tool_name: &str) -> Option<Arc<PluginToolSummary>> {
        self.tool_summary_registry.get(tool_name)
    }

    pub fn all_tool_summaries(&self) -> Vec<(String, String)> {
        self.tool_summary_registry.all()
    }

    /// Markdown rules in registration order for one surface (empty
    /// surface list means every surface). The host applies the returned
    /// rules in order and keeps them display-only.
    pub fn markdown_rules_for(&self, surface: &str) -> Vec<PluginMarkdownRule> {
        let mut rules: Vec<PluginMarkdownRule> = self
            .markdown_transformer_registry
            .all_with_values()
            .into_iter()
            .map(|(_, _, rule)| (*rule).clone())
            .filter(|rule| rule.surfaces.is_empty() || rule.surfaces.iter().any(|s| s == surface))
            .collect();
        rules.sort_by(|a, b| a.id.cmp(&b.id));
        rules
    }

    /// Owned Markdown rule snapshot covering every surface (the
    /// per-surface `markdown_rules_for` filters at render time instead).
    pub fn markdown_transformer_snapshot(&self) -> Vec<PluginMarkdownRule> {
        self.markdown_transformer_registry
            .all_with_values()
            .into_iter()
            .map(|(_, _, rule)| (*rule).clone())
            .collect()
    }

    pub fn get_markdown_transformer(&self, id: &str) -> Option<Arc<PluginMarkdownRule>> {
        self.markdown_transformer_registry.get(id)
    }

    pub fn all_markdown_transformers(&self) -> Vec<(String, String)> {
        self.markdown_transformer_registry.all()
    }
}
