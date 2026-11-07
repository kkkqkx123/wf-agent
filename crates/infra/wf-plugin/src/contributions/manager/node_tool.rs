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
}
