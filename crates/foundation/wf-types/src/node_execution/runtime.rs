use crate::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeNodeStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "node_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeNode {
    Start(RuntimeStartNode),
    End(RuntimeEndNode),
    Variable(RuntimeVariableNode),
    Fork(RuntimeForkNode),
    Join(RuntimeJoinNode),
    Sync(RuntimeSyncNode),
    Subgraph(RuntimeSubgraphNode),
    Script(RuntimeScriptNode),
    InteractiveScript(RuntimeInteractiveScriptNode),
    Llm(RuntimeLlmNode),
    ToolVisibility(RuntimeToolVisibilityNode),
    UserInteraction(RuntimeUserInteractionNode),
    Route(RuntimeRouteNode),
    ContextProcessor(RuntimeContextProcessorNode),
    LoopStart(RuntimeLoopStartNode),
    LoopEnd(RuntimeLoopEndNode),
    AgentLoop(RuntimeAgentLoopNode),
    StartFromMessage(RuntimeStartFromMessageNode),
    ContinueFromMessage(RuntimeContinueFromMessageNode),
    EmbedStart(RuntimeEmbedStartNode),
    EmbedEnd(RuntimeEmbedEndNode),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeStartNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::control::StartNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeEndNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::control::EndNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeVariableNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::variable::VariableNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeForkNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::fork_join::ForkNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeJoinNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::fork_join::JoinNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeSyncNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::sync::SyncNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeSubgraphNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::subgraph::SubgraphNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeScriptNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::script::ScriptNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeInteractiveScriptNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::script::InteractiveScriptNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeLlmNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::llm::LlmNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeToolVisibilityNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::tool_visibility::ToolVisibilityNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeUserInteractionNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::interaction::UserInteractionNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeRouteNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::control::RouteNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeContextProcessorNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::context::ContextProcessorNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeLoopStartNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::r#loop::LoopStartNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeLoopEndNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::r#loop::LoopEndNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeAgentLoopNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<crate::node::configs::agent_loop::AgentLoopNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeStartFromMessageNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeContinueFromMessageNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeEmbedStartNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeEmbedEndNode {
    pub id: crate::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}
