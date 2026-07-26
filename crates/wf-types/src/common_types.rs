pub type Id = String;
pub type Version = String;
pub type Metadata = HashMap<String, serde_json::Value>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VariableDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionHook {
    pub event: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HookType {
    #[serde(rename = "before_execution")]
    BeforeExecution,
    #[serde(rename = "after_execution")]
    AfterExecution,
    #[serde(rename = "on_error")]
    OnError,
    #[serde(rename = "on_completion")]
    OnCompletion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeHook {
    #[serde(rename = "type")]
    pub hook_type: HookType,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
}
