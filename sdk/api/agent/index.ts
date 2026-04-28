/**
 * Agent API Module Portal
 * Export all Agent-related APIs
 */

// ============================================================================
// Commands - Command Operations
// ============================================================================

// execute a command
export {
  RunAgentLoopCommand,
  type RunAgentLoopParams,
} from "./operations/run-agent-loop-command.js";
export {
  RunAgentLoopStreamCommand,
  type RunAgentLoopStreamParams,
} from "./operations/run-agent-loop-stream-command.js";

// control
export {
  CancelAgentLoopCommand,
  type CancelAgentLoopParams,
} from "./operations/cancel-agent-loop-command.js";
export {
  PauseAgentLoopCommand,
  type PauseAgentLoopParams,
} from "./operations/pause-agent-loop-command.js";
export {
  ResumeAgentLoopCommand,
  type ResumeAgentLoopParams,
} from "./operations/resume-agent-loop-command.js";

// checkpoint command
export {
  CreateCheckpointCommand,
  type CreateCheckpointParams,
} from "./operations/checkpoints/create-checkpoint-command.js";
export {
  RestoreCheckpointCommand,
  type RestoreCheckpointParams,
} from "./operations/checkpoints/restore-checkpoint-command.js";

// ============================================================================
// Subscriptions - Event Subscriptions
// ============================================================================

export {
  OnEventSubscription,
  type OnAgentEventParams,
} from "./operations/subscriptions/events/on-event-subscription.js";

export {
  OffEventSubscription,
  type OffAgentEventParams,
} from "./operations/subscriptions/events/off-event-subscription.js";

export {
  OnceEventSubscription,
  type OnceAgentEventParams,
} from "./operations/subscriptions/events/once-event-subscription.js";

// ============================================================================
// Resources - Resources API
// ============================================================================

export {
  AgentLoopRegistryAPI,
  type AgentLoopFilter,
  type AgentLoopSummary,
} from "./resources/agent-loop-registry-api.js";
export {
  AgentLoopCheckpointResourceAPI,
  type AgentLoopCheckpointFilter,
  type CheckpointStorage,
} from "./resources/checkpoint-resource-api.js";
export {
  AgentLoopMessageResourceAPI,
  type AgentLoopMessageFilter,
  type AgentLoopMessageStats,
} from "./resources/message-resource-api.js";
export {
  AgentLoopVariableResourceAPI,
  type AgentLoopVariableFilter,
  type VariableDefinition,
} from "./resources/variable-resource-api.js";
