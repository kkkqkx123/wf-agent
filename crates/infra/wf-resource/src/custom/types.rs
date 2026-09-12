use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct CustomToolDefinition {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: CustomToolType,
    pub description: String,
    pub schema: CustomParamSchema,
    pub handler: CustomHandlerConfig,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CustomToolType {
    Stateless,
    Stateful,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomParamSchema {
    pub parameters: Vec<CustomParamDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomParamDef {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum CustomHandlerConfig {
    #[serde(rename = "file")]
    File { path: String },
    #[serde(rename = "inline")]
    Inline { code: String },
    #[serde(rename = "rpc")]
    Rpc {
        endpoint: String,
        method: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomTriggerDefinition {
    pub name: String,
    pub description: String,
    pub condition: CustomTriggerCondition,
    /// Action executed when the condition matches. A trigger without an
    /// action can never do anything and is rejected at registration
    /// ("register-success-but-never-fires" is not allowed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<wf_types::trigger::TriggerAction>,
    /// Priority within the event competition scope. Only meaningful under
    /// the best-win dispatch mode, where every subscriber of one scope must
    /// declare a distinct priority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    /// Dispatch mode for the event scope this trigger subscribes to.
    /// Absent means the default unique dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_mode: Option<wf_types::trigger::TriggerDispatchMode>,
    /// Multi-effect opt-in (see `TriggerTemplate::allow_multi_effect`).
    /// Default keeps single-winner single-execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_multi_effect: Option<bool>,
    /// Explicit effect order required when `allow_multi_effect` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_order: Option<Vec<String>>,
    pub config: Option<Value>,
    pub metadata: Option<Value>,
}

/// Trigger source declared by custom resources. Only `Event` is executed;
/// `Schedule` and `Webhook` map to the reserved `TriggerSource` variants and
/// are rejected at registration until a scheduler or gateway exists.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum CustomTriggerCondition {
    #[serde(rename = "event")]
    Event { value: String },
    #[serde(rename = "schedule")]
    Schedule { value: String },
    #[serde(rename = "webhook")]
    Webhook { value: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomPromptDefinition {
    pub id: String,
    pub name: String,
    pub content: String,
    #[serde(rename = "type")]
    pub prompt_type: CustomPromptType,
    pub variables: Option<Vec<CustomPromptVariable>>,
    /// Fragment ids composed into the `{{fragments}}` placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragments: Option<Vec<String>>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomPromptType {
    System,
    User,
    Assistant,
    /// Composition-style template rendered from declared fragments.
    Fragments,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomPromptVariable {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: String,
    pub required: Option<bool>,
    pub description: Option<String>,
    /// Value used when the caller does not supply the variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
}

#[derive(Debug, Default)]
pub struct CustomResources {
    pub tools: Vec<CustomToolDefinition>,
    pub triggers: Vec<CustomTriggerDefinition>,
    pub prompts: Vec<CustomPromptDefinition>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomValidationLevel {
    /// Fail the whole custom-resource pipeline if any file fails to load or
    /// any definition fails validation (no partial registration).
    Strict,
    /// Register whatever loaded successfully; collect errors without aborting.
    #[default]
    Lenient,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomResourcesPresetConfig {
    pub enabled: Option<bool>,
    pub tools_path: Option<String>,
    pub triggers_path: Option<String>,
    pub prompts_path: Option<String>,
    pub validation_level: Option<CustomValidationLevel>,
}
