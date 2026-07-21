/**
 * Agent API Module Portal
 * Export all Agent-related APIs
 */

// ============================================================================
// Builders - Configuration Builders
// ============================================================================

export {
  AgentLoopConfigBuilder,
  AgentDefinitionBuilder,
  AgentToolConfigBuilder,
  AgentHookBuilder,
  AgentTriggerBuilder,
} from "./builders/index.js";

// ============================================================================
// Commands - Command Operations
// ============================================================================

// execution commands
export {
  RunAgentLoopCommand,
  type RunAgentLoopParams,
} from "./operations/run-agent-loop-command.js";
export {
  RunAgentLoopStreamCommand,
  type RunAgentLoopStreamParams,
} from "./operations/run-agent-loop-stream-command.js";

// control commands
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

// checkpoint commands
export {
  CreateCheckpointCommand,
  type CreateCheckpointParams,
} from "./operations/checkpoints/create-checkpoint-command.js";
export {
  RestoreCheckpointCommand,
  type RestoreCheckpointParams,
} from "./operations/checkpoints/restore-checkpoint-command.js";

// trigger commands
export {
  EnableAgentTriggerCommand,
  type EnableAgentTriggerParams,
} from "./operations/triggers/enable-agent-trigger-command.js";
export {
  DisableAgentTriggerCommand,
  type DisableAgentTriggerParams,
} from "./operations/triggers/disable-agent-trigger-command.js";

// ============================================================================
// Subscriptions - Event Subscriptions (from shared types)
// ============================================================================

export {
  OnEventSubscription,
  OnceEventSubscription,
  WaitForEventSubscription,
  createExecutionScopedSubscription,
  createExecutionScopedOnceSubscription,
} from "../shared/types/subscription.js";

// ============================================================================
// Resources - Resources API
// ============================================================================

export {
  AgentLoopRegistryAPI,
  type AgentLoopFilter,
  type AgentLoopSummary,
} from "./resources/agent-loop-registry-api.js";
export {
  /** @deprecated Use {@link AgentLoopRegistryAPI} instead */
  AgentLoopResourceAPI,
  type AgentLoopFilter as AgentLoopEntityFilter,
  type AgentLoopSummary as AgentLoopEntitySummary,
  type AgentLoopStorage,
} from "./resources/agent-loop-resource-api.js";
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
  AgentVariableResourceAPI,
  type AgentVariableFilter,
  type AgentVariableUpdateOptions,
  type AgentVariableDefinition,
  type AgentContextVariable,
} from "./resources/agent-variable-resource-api.js";

export {
  AgentUserInteractionResourceAPI,
  type AgentUserInteractionConfig,
  type AgentUserInteractionFilter,
  type AgentUserInteractionEventRecord,
} from "./resources/agent-user-interaction-resource-api.js";

export {
  AgentLoopIterationAPI,
  type ExtendedIterationDetail,
  type ExtendedIterationHistorySummary,
  type DecisionOutcome,
  type ToolDependency,
  type ExecutionPath,
  type LLMReasoningRecord,
  type ErrorContextRecord,
  type IterationSystemMetrics,
  type IterationLLMMetrics,
  type ExtendedIterationFilter,
  type DecisionAnalysis,
  type ExecutionPathAnalysis,
} from "./resources/agent-loop-iteration-api.js";

export {
  AgentExecutionStateAPI,
  type AgentExecutionState,
  type InputState,
  type OutputState,
  type VariableSnapshot,
  type VariableStateSnapshot,
  type ExecutionContextSnapshot,
  type StackFrame,
  type StateTransition,
  type ExecutionTimelineEntry,
  type ExecutionStateFilter,
  type VariableSnapshotFilter,
  type StateTransitionAnalysis,
} from "./resources/agent-execution-state-api.js";

export {
  AgentExecutionGraphQueryAPI,
  type DecisionNode,
  type DecisionEdge,
  type DecisionGraph,
  type ExecutionPathStep,
  type ExecutionPath as ExecutionGraphPath,
  type AlternativeDecision,
  type IterationAlternatives,
  type DecisionRecord,
  type DecisionSequence,
  type DecisionGraphFilter,
  type ExecutionPathFilter,
  type DecisionSequenceFilter,
} from "./resources/agent-execution-graph-query-api.js";

export {
  AgentTriggerResourceAPI,
  type AgentTriggerFilter,
} from "./resources/agent-trigger-resource-api.js";

export {
  AgentFileCheckpointResourceAPI,
  type AgentFileCheckpointFilter,
} from "./resources/agent-file-checkpoint-resource-api.js";

export {
  AgentTriggerTemplateRegistryAPI,
  type AgentTriggerTemplateFilter,
  type AgentTriggerTemplateSummary,
} from "./resources/agent-trigger-template-registry-api.js";

export {
  AgentHookTemplateRegistryAPI,
  type AgentHookTemplateFilter,
  type AgentHookTemplateSummary,
} from "./resources/agent-hook-template-registry-api.js";

export {
  AgentTemplateRegistryAPI,
  type AgentTemplateFilter,
  type AgentTemplateSummary,
} from "./resources/agent-template-registry-api.js";

// errors
export {
  AgentErrorAnalysisAPI,
  type RootCauseAnalysis,
  type ErrorStatistics,
  type ErrorRecoveryProposal,
} from "./resources/errors/index.js";
