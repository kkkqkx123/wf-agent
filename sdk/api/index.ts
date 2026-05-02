/**
 * API Layer Entry Files
 * Export all API modules and types
 *
 * directory structure:
 * - workflow/ : Workflow related APIs (Workflow, Execution, Checkpoint, etc.)
 * - agent/ : Agent related APIs (AgentLoop, etc.)
 * - shared/ : shared modules (types, core, common, utils, validation, etc.)
 */

// ============================================================================
// Shared - Command mode core
// ============================================================================
export {
  Command,
  BaseCommand,
  SyncCommand,
  BaseSyncCommand,
  CommandMetadata,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "./shared/types/command.js";

export { CommandExecutor } from "./shared/common/command-executor.js";

// Query pattern core
export {
  Query,
  BaseQuery,
  QueryMetadata,
  QueryResult,
  querySuccess,
  queryFailure,
  isQuerySuccess,
  isQueryFailure,
} from "./shared/types/query.js";

// The core of the Subscription model
export {
  Subscription,
  BaseSubscription,
  SubscriptionMetadata,
} from "./shared/types/subscription.js";

// Harmonization of types
export {
  ExecutionResult,
  success,
  failure,
  isSuccess,
  isFailure,
  getData,
  getError,
} from "./shared/types/execution-result.js";
export {
  ExecutionOptions,
  DEFAULT_EXECUTION_OPTIONS,
  mergeExecutionOptions,
} from "./shared/types/execution-options.js";

// Global SDK Example
export { getSDK } from "./shared/core/sdk.js";

// SDK Types
export type { SDKOptions, SDKLifecycleHooks } from "./shared/types/core-types.js";

// API Factory
export { APIFactory, getAPIFactory, type AllAPIs } from "./shared/core/api-factory.js";

// ============================================================================
// Shared - Resource Management API (CRUD Operations) - Shared Resources
// ============================================================================
export { ToolRegistryAPI } from "./shared/resources/tools/tool-registry-api.js";
export { ScriptRegistryAPI } from "./shared/resources/scripts/script-registry-api.js";
export { LLMProfileRegistryAPI as ProfileRegistryAPI } from "./shared/resources/llm/llm-profile-registry-api.js";
export {
  SkillRegistryAPI,
  type SkillFilter,
  type SkillLoadOptions,
} from "./shared/resources/skills/skill-registry-api.js";

// Event Resource API - Shared across all modules
export {
  EventResourceAPI,
  type EventFilter,
  type EventStats,
} from "./shared/resources/events/event-resource-api.js";

// Common Resource API Base Classes and Tools
export { ReadonlyResourceAPI, CrudResourceAPI } from "./shared/resources/generic-resource-api.js";
export { createSharedResourceAPIs, type SharedResourceAPIs } from "./shared/resources/index.js";

// ============================================================================
// Workflow - Resource Management API (CRUD Operations)
// ============================================================================
export { WorkflowRegistryAPI } from "./workflow/resources/workflows/workflow-registry-api.js";
export {
  WorkflowExecutionRegistryAPI,
  type WorkflowExecutionFilter,
  type WorkflowExecutionSummary,
} from "./workflow/resources/executions/workflow-execution-registry-api.js";
export { NodeRegistryAPI } from "./workflow/resources/templates/node-template-registry-api.js";
export { TriggerTemplateRegistryAPI } from "./workflow/resources/templates/trigger-template-registry-api.js";
export { CheckpointResourceAPI } from "./workflow/resources/checkpoints/checkpoint-resource-api.js";
export {
  MessageResourceAPI,
  type MessageFilter as WorkflowMessageFilter,
  type MessageStats,
} from "./workflow/resources/messages/message-resource-api.js";
export {
  VariableResourceAPI,
  type VariableDefinition,
  type VariableFilter,
} from "./workflow/resources/variables/variable-resource-api.js";
export { TriggerResourceAPI } from "./workflow/resources/triggers/trigger-resource-api.js";
export {
  UserInteractionResourceAPI,
  type UserInteractionConfig,
  type UserInteractionFilter,
} from "./workflow/resources/user-interaction/user-interaction-resource-api.js";
export {
  HumanRelayResourceAPI,
  type HumanRelayConfig,
  type HumanRelayFilter,
} from "./workflow/resources/human-relay/human-relay-resource-api.js";

// ============================================================================
// Workflow - Command class (with side-effect operations)
// ============================================================================

// Execution Commands
export { ExecuteWorkflowCommand } from "./workflow/operations/execution/execute-workflow-command.js";
export type { ExecuteWorkflowParams } from "./workflow/operations/execution/execute-workflow-command.js";

export { ExecuteWorkflowStreamCommand } from "./workflow/operations/execution/execute-workflow-stream-command.js";
export type { ExecuteWorkflowStreamParams } from "./workflow/operations/execution/execute-workflow-stream-command.js";

export { PauseWorkflowCommand } from "./workflow/operations/execution/pause-workflow-command.js";

export { ResumeWorkflowCommand } from "./workflow/operations/execution/resume-workflow-command.js";

export { CancelWorkflowCommand } from "./workflow/operations/execution/cancel-workflow-command.js";

// Checkpoint Commands
export { CreateCheckpointCommand as WorkflowCreateCheckpointCommand } from "./workflow/operations/checkpoints/create-checkpoint-command.js";
export type { CreateCheckpointParams as WorkflowCreateCheckpointParams } from "./workflow/operations/checkpoints/create-checkpoint-command.js";

export { RestoreFromCheckpointCommand } from "./workflow/operations/checkpoints/restore-from-checkpoint-command.js";
export type { RestoreFromCheckpointParams } from "./workflow/operations/checkpoints/restore-from-checkpoint-command.js";

// Trigger Commands
export { EnableTriggerCommand } from "./workflow/operations/triggers/enable-trigger-command.js";
export type { EnableTriggerParams } from "./workflow/operations/triggers/enable-trigger-command.js";

export { DisableTriggerCommand } from "./workflow/operations/triggers/disable-trigger-command.js";
export type { DisableTriggerParams } from "./workflow/operations/triggers/disable-trigger-command.js";

// Subscriptions
export { OnEventSubscription } from "./workflow/operations/subscriptions/events/on-event-subscription.js";
export type { OnEventParams } from "./workflow/operations/subscriptions/events/on-event-subscription.js";

export { OnceEventSubscription } from "./workflow/operations/subscriptions/events/once-event-subscription.js";
export type { OnceEventParams } from "./workflow/operations/subscriptions/events/once-event-subscription.js";

export { OffEventSubscription } from "./workflow/operations/subscriptions/events/off-event-subscription.js";
export type { OffEventParams } from "./workflow/operations/subscriptions/events/off-event-subscription.js";

// ============================================================================
// Shared - Command class (with side-effect operations)
// ============================================================================

// Event Commands - Shared across all modules
export { DispatchEventCommand } from "./shared/operations/events/dispatch-event-command.js";
export type { DispatchEventParams } from "./shared/operations/events/dispatch-event-command.js";

// ============================================================================
// Workflow - Builder
// ============================================================================
export { WorkflowBuilder, ExecutionBuilder } from "./workflow/builders/index.js";
export { NodeTemplateBuilder } from "./workflow/builders/node-template-builder.js";
export { TriggerTemplateBuilder } from "./workflow/builders/trigger-template-builder.js";

// ============================================================================
// Workflow - Validation API
// ============================================================================
export { WorkflowValidator as WorkflowValidatorAPI } from "../workflow/validation/index.js";
export { CodeConfigValidator as CodeConfigValidatorAPI } from "../workflow/validation/script-config-validator.js";
export { StaticValidator as StaticValidatorAPI } from "../core/validation/index.js";
export { StaticValidator } from "../core/validation/index.js";
export { RuntimeValidator as RuntimeValidatorAPI } from "../core/validation/index.js";
export {
  validateHook as validateHookAPI,
  validateHooks as validateHooksAPI,
} from "../core/validation/index.js";
export {
  validateTriggerCondition as validateTriggerConditionAPI,
  validateExecuteTriggeredSubgraphActionConfig as validateExecuteTriggeredSubgraphActionConfigAPI,
  validateTriggerAction as validateTriggerActionAPI,
  validateWorkflowTrigger as validateWorkflowTriggerAPI,
  validateTriggerReference as validateTriggerReferenceAPI,
  validateTriggers as validateTriggersAPI,
} from "../core/validation/index.js";

// ============================================================================
// Workflow - Hook Creator
// ============================================================================
export {
  createWorkflowExecutionStateCheckHook,
  createPermissionCheckHook,
  createAuditLoggingHook,
} from "../workflow/execution/utils/hook-creators.js";

// ============================================================================
// Shared - Commands (LLM, Tool, Script)
// ============================================================================

// LLM Commands
export { GenerateCommand } from "./shared/operations/generate-command.js";

export { GenerateBatchCommand } from "./shared/operations/generate-batch-command.js";

// Script Commands
export { ExecuteScriptCommand } from "./shared/operations/scripts/execute-script-command.js";

// Tool Commands
export { ExecuteToolCommand } from "./shared/operations/tools/execute-tool-command.js";

// ============================================================================
// Agent - Commands
// ============================================================================
export {
  RunAgentLoopCommand,
  type RunAgentLoopParams,
} from "./agent/operations/run-agent-loop-command.js";

export {
  RunAgentLoopStreamCommand,
  type RunAgentLoopStreamParams,
} from "./agent/operations/run-agent-loop-stream-command.js";

export {
  CancelAgentLoopCommand,
  type CancelAgentLoopParams,
} from "./agent/operations/cancel-agent-loop-command.js";

export {
  PauseAgentLoopCommand,
  type PauseAgentLoopParams,
} from "./agent/operations/pause-agent-loop-command.js";

export {
  ResumeAgentLoopCommand,
  type ResumeAgentLoopParams,
} from "./agent/operations/resume-agent-loop-command.js";

// Agent Checkpoint Commands
export {
  CreateCheckpointCommand,
  type CreateCheckpointParams,
} from "./agent/operations/checkpoints/create-checkpoint-command.js";

export {
  RestoreCheckpointCommand,
  type RestoreCheckpointParams,
} from "./agent/operations/checkpoints/restore-checkpoint-command.js";

// Agent Event Subscriptions
export {
  OnEventSubscription as AgentOnEventSubscription,
  type OnAgentEventParams,
} from "./agent/operations/subscriptions/events/on-event-subscription.js";

export {
  OffEventSubscription as AgentOffEventSubscription,
  type OffAgentEventParams,
} from "./agent/operations/subscriptions/events/off-event-subscription.js";

export {
  OnceEventSubscription as AgentOnceEventSubscription,
  type OnceAgentEventParams,
} from "./agent/operations/subscriptions/events/once-event-subscription.js";

// ============================================================================
// Agent - Resources
// ============================================================================
export {
  AgentLoopRegistryAPI,
  type AgentLoopFilter,
  type AgentLoopSummary,
} from "./agent/resources/agent-loop-registry-api.js";

export {
  AgentLoopResourceAPI,
  type AgentLoopFilter as AgentLoopEntityFilter,
  type AgentLoopSummary as AgentLoopEntitySummary,
  type AgentLoopStorage,
} from "./agent/resources/agent-loop-resource-api.js";

export {
  AgentLoopCheckpointResourceAPI,
  type AgentLoopCheckpointFilter,
  type CheckpointStorage,
} from "./agent/resources/checkpoint-resource-api.js";

export {
  AgentLoopMessageResourceAPI,
  type AgentLoopMessageFilter,
  type AgentLoopMessageStats,
} from "./agent/resources/message-resource-api.js";

export {
  AgentLoopVariableResourceAPI,
  type AgentLoopVariableFilter,
} from "./agent/resources/variable-resource-api.js";

// ============================================================================
// Shared - Types
// ============================================================================
export type { CommandError } from "./shared/types/command-error.js";

// ============================================================================
// Shared - Utilities
// ============================================================================
export { Observable, create, type Observer } from "./shared/utils/observable.js";

// ============================================================================
// Shared - Component Message
// ============================================================================
export {
  MessageBus,
  type MessageFilter,
  type MessageHandler,
  type MessageSubscription,
} from "./shared/component-message/message-bus.js";

export {
  MessagePublisher,
  createMessagePublisher,
} from "./shared/component-message/publisher-api.js";

export {
  matchesRoutingRule,
  findMatchingRule,
  sortRulesByPriority,
} from "./shared/component-message/routing-utils.js";

// ============================================================================
// Shared - Configuration Parsing Utilities
// ============================================================================
export {
  parseToml,
  parseJson,
  parseWorkflow,
  parseNodeTemplate,
  parseTriggerTemplate,
  parseScript,
  parseLLMProfile,
  parseAndValidateAgentLoopConfig,
} from "./shared/config/index.js";
