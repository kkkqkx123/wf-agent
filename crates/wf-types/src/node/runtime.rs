use serde::{Deserialize, Serialize};
use super::super::Timestamp;

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
    EmbedGraph(RuntimeEmbedGraphNode),
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
    StartFromTrigger(RuntimeStartFromTriggerNode),
    ContinueFromTrigger(RuntimeContinueFromTriggerNode),
    EmbedStart(RuntimeEmbedStartNode),
    EmbedEnd(RuntimeEmbedEndNode),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeStartNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::control::StartNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeEndNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::control::EndNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeVariableNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::variable::VariableNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeForkNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::fork_join::ForkNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeJoinNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::fork_join::JoinNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeSyncNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::sync::SyncNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeSubgraphNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::subgraph::SubgraphNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeEmbedGraphNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeScriptNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::script::ScriptNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeInteractiveScriptNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::script::InteractiveScriptNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeLlmNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::llm::LlmNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeToolVisibilityNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::tool_visibility::ToolVisibilityNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeUserInteractionNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::interaction::UserInteractionNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeRouteNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::control::RouteNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeContextProcessorNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::context::ContextProcessorNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeLoopStartNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::r#loop::LoopStartNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeLoopEndNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::r#loop::LoopEndNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeAgentLoopNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub config: Option<super::configs::agent_loop::AgentLoopNodeOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeStartFromTriggerNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeContinueFromTriggerNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeEmbedStartNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeEmbedEndNode {
    pub id: super::super::Id,
    pub status: RuntimeNodeStatus,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}
