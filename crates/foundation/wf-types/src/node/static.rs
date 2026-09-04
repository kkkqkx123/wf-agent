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

impl StaticNodeType {
    /// All known node type variant names in SCREAMING_SNAKE_CASE.
    pub const ALL: &'static [&'static str] = &[
        "START",
        "END",
        "VARIABLE",
        "LLM",
        "SCRIPT",
        "TOOL_VISIBILITY",
        "FORK",
        "JOIN",
        "SYNC",
        "LOOP_START",
        "LOOP_END",
        "ROUTE",
        "SUBGRAPH",
        "EMBED_GRAPH",
        "AGENT_LOOP",
        "INTERACTIVE_SCRIPT",
        "USER_INTERACTION",
        "START_FROM_MESSAGE",
        "CONTINUE_FROM_MESSAGE",
        "EMBED_START",
        "EMBED_END",
        "CONTEXT_PROCESSOR",
    ];

    /// Parse a node type string (case-insensitive). Returns `None` for
    /// unknown values.
    pub fn from_str_ci(value: &str) -> Option<Self> {
        match value.to_uppercase().as_str() {
            "START" => Some(Self::Start),
            "END" => Some(Self::End),
            "EMBED_START" => Some(Self::EmbedStart),
            "EMBED_END" => Some(Self::EmbedEnd),
            "VARIABLE" => Some(Self::Variable),
            "FORK" => Some(Self::Fork),
            "JOIN" => Some(Self::Join),
            "SYNC" => Some(Self::Sync),
            "SUBGRAPH" => Some(Self::Subgraph),
            "EMBED_GRAPH" => Some(Self::EmbedGraph),
            "SCRIPT" => Some(Self::Script),
            "INTERACTIVE_SCRIPT" => Some(Self::InteractiveScript),
            "LLM" => Some(Self::Llm),
            "TOOL_VISIBILITY" => Some(Self::ToolVisibility),
            "USER_INTERACTION" => Some(Self::UserInteraction),
            "ROUTE" => Some(Self::Route),
            "CONTEXT_PROCESSOR" => Some(Self::ContextProcessor),
            "LOOP_START" => Some(Self::LoopStart),
            "LOOP_END" => Some(Self::LoopEnd),
            "AGENT_LOOP" => Some(Self::AgentLoop),
            "START_FROM_MESSAGE" => Some(Self::StartFromMessage),
            "CONTINUE_FROM_MESSAGE" => Some(Self::ContinueFromMessage),
            _ => None,
        }
    }
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
