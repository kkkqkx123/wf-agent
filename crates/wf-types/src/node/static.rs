use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StaticNodeType {
    Start,
    End,
    EmbedStart,
    EmbedEnd,
    Variable,
    Fork,
    Join,
    Sync,
    Subgraph,
    EmbedGraph,
    Script,
    InteractiveScript,
    Llm,
    ToolVisibility,
    UserInteraction,
    Route,
    ContextProcessor,
    LoopStart,
    LoopEnd,
    AgentLoop,
    StartFromMessage,
    ContinueFromMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaseStaticNode {
    pub id: super::super::Id,
    pub node_type: StaticNodeType,
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_config: Option<super::NodeExecutionConfig>,
}
