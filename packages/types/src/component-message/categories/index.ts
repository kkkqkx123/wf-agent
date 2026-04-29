/**
 * Message Categories Index
 *
 * Exports all message category types and enums.
 */

// System messages
export {
  SystemMessageType,
  type SystemStartupData,
  type SystemShutdownData,
  type SystemConfigChangeData,
  type SystemErrorData,
} from "./system.js";

// Workflow Execution messages
export {
  WorkflowExecutionMessageType,
  type WorkflowExecutionStartData,
  type WorkflowExecutionEndData,
  type WorkflowExecutionNodeData,
  type WorkflowExecutionVariableData,
  type WorkflowExecutionForkData,
  type WorkflowExecutionForkBranchData,
  type WorkflowExecutionJoinData,
  type WorkflowExecutionAgentCallData,
  type WorkflowExecutionAgentReturnData,
  type WorkflowExecutionSubgraphCallData,
  type WorkflowExecutionSubgraphReturnData,
} from "./workflow-execution.js";

// Agent messages
export {
  AgentMessageType,
  type AgentStartData,
  type AgentEndData,
  type AgentIterationData,
  type AgentLLMRequestData,
  type AgentLLMStreamData,
  type AgentLLMResponseData,
  type AgentLLMErrorData,
  type AgentToolCallData,
  type AgentToolEndData,
  type AgentToolResultData,
  type AgentToolErrorData,
  type AgentHumanRelayRequestData,
  type AgentHumanRelayResponseData,
  type AgentCheckpointData,
  type AgentMessageAddData,
} from "./agent.js";

// Tool messages
export {
  ToolMessageType,
  type ToolCallStartData,
  type ToolCallEndData,
  type ToolResultData,
  type ToolErrorData,
} from "./tool.js";

// Human Relay messages
export {
  HumanRelayMessageType,
  type HumanRelayRequestData,
  type HumanRelayResponseData,
  type HumanRelayTimeoutData,
  type HumanRelayCancelData,
} from "./human-relay.js";

// Subgraph messages
export {
  SubgraphMessageType,
  type SubgraphStartData,
  type SubgraphEndData,
  type SubgraphContextInheritData,
  type SubgraphContextReturnData,
  type SubgraphStateSyncData,
} from "./subgraph.js";

// Checkpoint messages
export {
  CheckpointMessageType,
  type CheckpointCreateData,
  type CheckpointRestoreData,
  type CheckpointDeleteData,
  type CheckpointFailData,
} from "./checkpoint.js";

// Event messages
export {
  EventMessageType,
  type EventTriggerData,
  type EventProcessStartData,
  type EventProcessEndData,
  type EventCustomData,
} from "./event.js";
