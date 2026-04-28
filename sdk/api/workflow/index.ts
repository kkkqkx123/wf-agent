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
} from "./resources/executions/index.js";

export { WorkflowRegistryAPI, type WorkflowFilter, type WorkflowSummary } from "./resources/workflows/workflow-registry-api.js";

export {
  MessageResourceAPI,
  type MessageFilter,
  type MessageStats,
} from "./resources/messages/message-resource-api.js";

export {
  VariableResourceAPI,
  type VariableFilter,
  type VariableDefinition,
} from "./resources/variables/variable-resource-api.js";

export { TriggerResourceAPI, type TriggerFilter } from "./resources/triggers/trigger-resource-api.js";

export { CheckpointResourceAPI } from "./resources/checkpoints/checkpoint-resource-api.js";

export {
  NodeRegistryAPI,
  type NodeTemplateFilter,
  type NodeTemplateSummary,
} from "./resources/templates/node-template-registry-api.js";

export {
  TriggerTemplateRegistryAPI,
  type TriggerTemplateFilter,
  type TriggerTemplateSummary,
} from "./resources/templates/trigger-template-registry-api.js";

export {
  UserInteractionResourceAPI,
  type UserInteractionConfig,
  type UserInteractionFilter,
} from "./resources/user-interaction/user-interaction-resource-api.js";

export {
  HumanRelayResourceAPI,
  type HumanRelayConfig,
  type HumanRelayFilter,
} from "./resources/human-relay/human-relay-resource-api.js";

// Operations - Execution
export {
  ExecuteWorkflowCommand,
  type ExecuteWorkflowParams,
  ExecuteWorkflowStreamCommand,
  type ExecuteWorkflowStreamParams,
  type WorkflowStreamEvent,
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

// Operations - Subscriptions
export { OnEventSubscription } from "./operations/subscriptions/events/on-event-subscription.js";
export { OffEventSubscription } from "./operations/subscriptions/events/off-event-subscription.js";
export { OnceEventSubscription } from "./operations/subscriptions/events/once-event-subscription.js";
