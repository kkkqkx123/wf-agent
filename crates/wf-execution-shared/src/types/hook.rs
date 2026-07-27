use wf_types::Id;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BaseHookDefinition {
    pub id: Id,
    pub hook_type: String,
    pub weight: i32,
    pub condition: Option<String>,
    pub enabled: bool,
    pub parallel: bool,
    pub continue_on_error: bool,
}

#[derive(Debug, Clone)]
pub struct BaseHookContext {
    pub execution_id: Id,
    pub data: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookExecutionResult {
    pub hook_id: Id,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct HookExecutorConfig {
    pub parallel: bool,
    pub continue_on_error: bool,
    pub warn_on_condition_failure: bool,
}
