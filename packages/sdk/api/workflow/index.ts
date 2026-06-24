/**
 * Workflow API Module
 * Provides all workflow-related APIs including execution, resources, and operations.
 */

// Builders
export {
  BaseBuilder,
  TemplateBuilder,
  WorkflowBuilder,
  ExecutionBuilder,
  NodeBuilder,
  NodeTemplateBuilder,
  TriggerTemplateBuilder,
} from "./builders/index.js";

// Resource APIs
export {
  WorkflowExecutionRegistryAPI,
  type WorkflowExecutionFilter,
  type WorkflowExecutionSummary,
} from "./resources/workflow-execution-registry-api.js";

export {
  WorkflowRegistryAPI,
  type WorkflowFilter,
  type WorkflowSummary,
} from "./resources/workflow-registry-api.js";

export {
  MessageResourceAPI,
  type MessageFilter,
  type MessageStats,
} from "./resources/message-resource-api.js";

export {
  VariableResourceAPI,
  type VariableFilter,
  type VariableDefinition,
} from "./resources/variable-resource-api.js";

export {
  TriggerResourceAPI,
  type TriggerFilter,
} from "./resources/trigger-resource-api.js";

export { CheckpointResourceAPI } from "./resources/checkpoint-resource-api.js";

export {
  NodeRegistryAPI,
  type NodeTemplateFilter,
  type NodeTemplateSummary,
} from "./resources/node-template-registry-api.js";

export {
  TriggerTemplateRegistryAPI,
  type TriggerTemplateFilter,
  type TriggerTemplateSummary,
} from "./resources/trigger-template-registry-api.js";

export {
  UserInteractionResourceAPI,
  type UserInteractionConfig,
  type UserInteractionFilter,
} from "./resources/user-interaction-resource-api.js";

export {
  WorkflowIterationAnalysisAPI,
  type ExtendedNodeExecutionRecord,
  type ToolDependency,
  type ExecutionPath,
  type ExecutionPathStep,
  type LLMNodeMetadata,
  type LLMReasoningRecord,
  type QualityMetrics,
  type ExtendedNodeExecutionFilter,
  type NodeExecutionStats,
  type OptimizationOpportunity,
} from "./resources/workflow-iteration-analysis-api.js";

export {
  WorkflowExecutionContextAPI,
  type VariableDefinitionWithScope,
  type VariableValueSnapshot,
  type VariableSnapshot,
  type VariableHistoryEntry,
  type VariableHistory,
  type NodeInputContext,
  type ExecutionContextSnapshot,
  type ContextStateTransition,
  type ContextEvolution,
  type VariableSnapshotFilter,
  type ContextEvolutionFilter,
} from "./resources/workflow-execution-context-api.js";

// Operations - Execution
export {
  ExecuteWorkflowCommand,
  type ExecuteWorkflowParams,
  ExecuteWorkflowStreamCommand,
  type ExecuteWorkflowStreamParams,
  PauseWorkflowCommand,
  ResumeWorkflowCommand,
  CancelWorkflowCommand,
} from "./operations/execution/index.js";

// Operations - Checkpoints
export { CreateCheckpointCommand } from "./operations/checkpoints/create-checkpoint-command.js";
export { RestoreFromCheckpointCommand } from "./operations/checkpoints/restore-from-checkpoint-command.js";

// Operations - Triggers
export { EnableTriggerCommand } from "./operations/triggers/enable-trigger-command.js";
export { DisableTriggerCommand } from "./operations/triggers/disable-trigger-command.js";

// Operations - Subscriptions (from shared types)
export {
  OnEventSubscription,
  OnceEventSubscription,
  WaitForEventSubscription,
  createExecutionScopedSubscription,
  createExecutionScopedOnceSubscription,
} from "../shared/types/subscription.js";
