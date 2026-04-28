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

// Thread messages
export {
  ThreadMessageType,
  type ThreadStartData,
  type ThreadEndData,
  type ThreadNodeData,
  type ThreadVariableData,
  type ThreadForkData,
  type ThreadForkBranchData,
  type ThreadJoinData,
  type ThreadAgentCallData,
  type ThreadAgentReturnData,
  type ThreadSubgraphCallData,
  type ThreadSubgraphReturnData,
} from "./thread.js";

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
