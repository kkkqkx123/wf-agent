use super::*;

impl ContributionManager {
    // --- Workflows ---
    pub fn all_workflows(&self) -> Vec<(String, String)> {
        self.workflow_registry.all()
    }
    pub fn get_workflow(&self, id: &str) -> Option<Arc<WorkflowTemplate>> {
        self.workflow_registry.get(id)
    }

    // --- Prompts ---
    pub fn all_prompts(&self) -> Vec<(String, String)> {
        self.prompt_registry.all()
    }
    pub fn get_prompt(&self, id: &str) -> Option<Arc<Template>> {
        self.prompt_registry.get(id)
    }

    // --- Fragments ---
    pub fn all_fragments(&self) -> Vec<(String, String)> {
        self.fragment_registry.all()
    }
    pub fn get_fragment(&self, id: &str) -> Option<Arc<SystemPromptFragment>> {
        self.fragment_registry.get(id)
    }

    // --- Agent templates ---
    pub fn all_agent_templates(&self) -> Vec<(String, String)> {
        self.agent_template_registry.all()
    }
    pub fn get_agent_template(&self, id: &str) -> Option<Arc<AgentTemplate>> {
        self.agent_template_registry.get(id)
    }

    // --- Node templates ---
    pub fn all_node_templates(&self) -> Vec<(String, String)> {
        self.node_template_registry.all()
    }
    pub fn get_node_template(&self, id: &str) -> Option<Arc<NodeTemplate>> {
        self.node_template_registry.get(id)
    }

    // --- Triggers ---
    pub fn all_triggers(&self) -> Vec<(String, String)> {
        self.trigger_registry.all()
    }
    pub fn get_trigger(&self, id: &str) -> Option<Arc<TriggerTemplate>> {
        self.trigger_registry.get(id)
    }

    // --- Tool descriptions ---
    pub fn all_tool_descriptions(&self) -> Vec<(String, String)> {
        self.tool_description_registry.all()
    }
    pub fn get_tool_description(&self, id: &str) -> Option<Arc<ToolDescriptionData>> {
        self.tool_description_registry.get(id)
    }

    // --- Tools ---
    pub fn all_tools(&self) -> Vec<(String, String)> {
        self.tool_registry.all()
    }
    pub fn get_tool(&self, id: &str) -> Option<Arc<ToolDef>> {
        self.tool_registry.get(id)
    }
}
