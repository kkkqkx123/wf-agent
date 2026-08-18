use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StartNodeOutput {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EndNodeOutput {
    pub output: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteNodeConfig {
    pub conditions: Vec<RouteCondition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_target_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteCondition {
    pub expression: String,
    pub target_node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteNodeOutput {
    pub next_node_id: String,
    pub evaluated_conditions: Vec<EvaluatedCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluatedCondition {
    pub expression: String,
    pub target_node_id: String,
    pub matched: bool,
}
