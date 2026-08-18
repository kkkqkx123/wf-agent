use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IndexType {
    LlmProfiles,
    Workflows,
    NodeTemplates,
    TriggerTemplates,
    Scripts,
    PromptTemplates,
    AgentLoops,
    McpPresets,
    SkillPresets,
    InfrastructurePresets,
}

impl IndexType {
    pub fn as_str(&self) -> &'static str {
        match self {
            IndexType::LlmProfiles => "llm_profiles",
            IndexType::Workflows => "workflows",
            IndexType::NodeTemplates => "node_templates",
            IndexType::TriggerTemplates => "trigger_templates",
            IndexType::Scripts => "scripts",
            IndexType::PromptTemplates => "prompt_templates",
            IndexType::AgentLoops => "agent_loops",
            IndexType::McpPresets => "mcp_presets",
            IndexType::SkillPresets => "skill_presets",
            IndexType::InfrastructurePresets => "infrastructure_presets",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFileFormat {
    Toml,
    Json,
}

/// Index file schema matching the `index.json` format: `version` +
/// `type` + `paths`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigIndexFile {
    pub version: String,
    #[serde(rename = "type")]
    pub index_type: IndexType,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedIndexEntry {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub category: Option<String>,
    pub file_path: String,
    pub format: ConfigFileFormat,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedLlmProfileEntry {
    #[serde(flatten)]
    pub base: ResolvedIndexEntry,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedWorkflowEntry {
    #[serde(flatten)]
    pub base: ResolvedIndexEntry,
    pub workflow_type: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedNodeTemplateEntry {
    #[serde(flatten)]
    pub base: ResolvedIndexEntry,
    pub node_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedScriptEntry {
    #[serde(flatten)]
    pub base: ResolvedIndexEntry,
    pub category: Option<String>,
    pub executor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedIndexMetadata {
    pub resolved_at: String,
    pub total_count: usize,
    pub failures: Vec<IndexLoadFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexLoadFailure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedIndex {
    pub index_type: IndexType,
    pub entries: Vec<ResolvedIndexEntry>,
    pub metadata: Option<ResolvedIndexMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillCollectionFile {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfrastructurePresetFile {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub files: InfrastructurePresetFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InfrastructurePresetFiles {
    pub metrics: Option<String>,
    pub timeout: Option<String>,
    pub storage: Option<String>,
    pub output: Option<String>,
    pub file_checkpoint: Option<String>,
    pub presets: Option<String>,
    pub tools: Option<String>,
    pub sandbox: Option<String>,
}
