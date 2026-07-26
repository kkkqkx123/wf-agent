use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteNodeConfig {
    pub conditions: Vec<RouteCondition>,
    pub default_route: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteCondition {
    pub expression: String,
    pub target_node_id: String,
}
