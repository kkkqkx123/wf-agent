/**
 * API Layer Entry Files
 * Export all API modules and types
 *
 * directory structure:
 * - graph/ : Graph related APIs (Workflow, Thread, Checkpoint, etc.)
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
export { createResourceAPIs, type ResourceAPIs } from "./shared/resources/index.js";

// ============================================================================
// Graph - Resource Management API (CRUD Operations)
// ============================================================================
export { WorkflowRegistryAPI } from "./graph/resources/workflows/workflow-registry-api.js";
export { ThreadRegistryAPI } from "./graph/resources/threads/thread-registry-api.js";
export { NodeRegistryAPI } from "./graph/resources/templates/node-template-registry-api.js";
export { TriggerTemplateRegistryAPI } from "./graph/resources/templates/trigger-template-registry-api.js";
export { CheckpointResourceAPI } from "./graph/resources/checkpoints/checkpoint-resource-api.js";
export {
  MessageResourceAPI,
  type MessageFilter,
  type MessageStats,
} from "./graph/resources/messages/message-resource-api.js";
export {
  VariableResourceAPI,
  type VariableDefinition,
  type VariableFilter,
} from "./graph/resources/variables/variable-resource-api.js";
export { TriggerResourceAPI } from "./graph/resources/triggers/trigger-resource-api.js";
export {
  UserInteractionResourceAPI,
  type UserInteractionConfig,
  type UserInteractionFilter,
} from "./graph/resources/user-interaction/user-interaction-resource-api.js";
export {
  HumanRelayResourceAPI,
  type HumanRelayConfig,
  type HumanRelayFilter,
} from "./graph/resources/human-relay/human-relay-resource-api.js";

// ============================================================================
// Graph - Command class (with side-effect operations)
// ============================================================================

// Execution Commands
export { ExecuteThreadCommand } from "./graph/operations/execution/execute-thread-command.js";
export type { ExecuteThreadParams } from "./graph/operations/execution/execute-thread-command.js";

export { ExecuteThreadStreamCommand } from "./graph/operations/execution/execute-thread-stream-command.js";
export type {
  ExecuteThreadStreamParams,
  ThreadStreamEvent,
} from "./graph/operations/execution/execute-thread-stream-command.js";

export { PauseThreadCommand } from "./graph/operations/execution/pause-thread-command.js";

export { ResumeThreadCommand } from "./graph/operations/execution/resume-thread-command.js";

export { CancelThreadCommand } from "./graph/operations/execution/cancel-thread-command.js";

// Checkpoint Commands
export { CreateCheckpointCommand as GraphCreateCheckpointCommand } from "./graph/operations/checkpoints/create-checkpoint-command.js";
export type { CreateCheckpointParams as GraphCreateCheckpointParams } from "./graph/operations/checkpoints/create-checkpoint-command.js";

export { RestoreFromCheckpointCommand } from "./graph/operations/checkpoints/restore-from-checkpoint-command.js";
export type { RestoreFromCheckpointParams } from "./graph/operations/checkpoints/restore-from-checkpoint-command.js";

// Trigger Commands
export { EnableTriggerCommand } from "./graph/operations/triggers/enable-trigger-command.js";
export type { EnableTriggerParams } from "./graph/operations/triggers/enable-trigger-command.js";

export { DisableTriggerCommand } from "./graph/operations/triggers/disable-trigger-command.js";
export type { DisableTriggerParams } from "./graph/operations/triggers/disable-trigger-command.js";

// Subscriptions
export { OnEventSubscription } from "./graph/operations/subscriptions/events/on-event-subscription.js";
export type { OnEventParams } from "./graph/operations/subscriptions/events/on-event-subscription.js";

export { OnceEventSubscription } from "./graph/operations/subscriptions/events/once-event-subscription.js";
export type { OnceEventParams } from "./graph/operations/subscriptions/events/once-event-subscription.js";

export { OffEventSubscription } from "./graph/operations/subscriptions/events/off-event-subscription.js";
export type { OffEventParams } from "./graph/operations/subscriptions/events/off-event-subscription.js";

// ============================================================================
// Shared - Command class (with side-effect operations)
// ============================================================================

// Event Commands - Shared across all modules
export { DispatchEventCommand } from "./shared/operations/events/dispatch-event-command.js";
export type { DispatchEventParams } from "./shared/operations/events/dispatch-event-command.js";

// ============================================================================
// Graph - Builder
// ============================================================================
export { WorkflowBuilder, ExecutionBuilder } from "./graph/builders/index.js";
export { NodeTemplateBuilder } from "./graph/builders/node-template-builder.js";
export { TriggerTemplateBuilder } from "./graph/builders/trigger-template-builder.js";

// ============================================================================
// Graph - Authentication API
// ============================================================================
export { WorkflowValidator as WorkflowValidatorAPI } from "../graph/validation/index.js";
export { CodeConfigValidator as CodeConfigValidatorAPI } from "../graph/validation/script-config-validator.js";
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
// Graph - Hook Creator
// ============================================================================
export {
  createThreadStateCheckHook,
  createCustomValidationHook,
  createPermissionCheckHook,
  createAuditLoggingHook,
} from "../graph/execution/utils/hook-creators.js";

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
  type VariableDefinition as AgentLoopVariableDefinition,
} from "./agent/resources/variable-resource-api.js";

// ============================================================================
// Agent - Core Components (from sdk/agent)
// ============================================================================
// physical layer (OSI)
export {
  AgentLoopEntity,
  AgentLoopState,
  AgentLoopStatus,
  type ToolCallRecord,
  type IterationRecord,
} from "../agent/entities/index.js";

// `AgentLoopStateSnapshot` is exported from `agent/index.ts`.
export { type AgentLoopStateSnapshot } from "../agent/index.js";

// manager layer
export { MessageHistory, type MessageHistoryState } from "../agent/message/index.js";
export { VariableState, type VariableStateSnapshot } from "../agent/variable/index.js";

// Execution layer (factory and life cycle)
export {
  AgentLoopFactory,
  createAgentLoopCheckpoint,
  cleanupAgentLoop,
  cloneAgentLoop,
  type AgentLoopEntityOptions,
  type AgentLoopCheckpointDependencies,
  type AgentLoopCheckpointOptions,
} from "../agent/execution/index.js";

// coordinator layer
export {
  AgentLoopCoordinator,
  type AgentLoopExecuteOptions,
} from "../agent/execution/coordinators/index.js";

// actuator layer
export {
  AgentLoopExecutor,
  type AgentLoopStreamEvent,
} from "../agent/execution/executors/index.js";

// checkpoint layer
export {
  AgentLoopDiffCalculator,
  AgentLoopDeltaRestorer,
  AgentLoopCheckpointResolver,
  AgentLoopCheckpointCoordinator,
  createCheckpoint,
  restoreFromCheckpoint,
  type CheckpointDependencies,
  type CheckpointOptions,
  type CreateCheckpointOptions,
} from "../agent/checkpoint/index.js";

// service layer (export class for direct instantiation, also available via DI)
export { AgentLoopRegistry } from "../agent/loop/agent-loop-registry.js";

// Hook handlers
export {
  executeAgentHook,
  type AgentHookExecutionContext,
  type AgentHookDefinition,
  buildAgentHookEvaluationContext,
  convertToEvaluationContext,
  type AgentHookEvaluationContext,
  emitAgentHookEvent,
  type AgentCustomEventData,
} from "../agent/execution/handlers/index.js";

// ============================================================================
// SDK Core - Core Components
// ============================================================================
// LLM Components
export { LLMWrapper, ProfileManager } from "../core/llm/index.js";
export { LLMExecutor } from "../core/executors/llm-executor.js";
export type {
  MessageStreamEvent,
  MessageStreamStreamEvent,
  MessageStreamTextEvent,
  MessageStreamInputJsonEvent,
  MessageStreamMessageEvent,
  MessageStreamFinalMessageEvent,
  MessageStreamErrorEvent,
  MessageStreamAbortEvent,
  MessageStreamEndEvent,
} from "../core/llm/index.js";

// Event Manager
export { EventRegistry } from "../core/registry/event-registry.js";

// Service Components
export { ToolRegistry } from "../core/registry/tool-registry.js";

// ============================================================================
// Shared - Tool Functions
// ============================================================================
// Result type - imported from the core layer
export { ok, err, all, any, tryCatchAsyncWithSignal } from "@wf-agent/common-utils";
export type { Result, Ok, Err } from "@wf-agent/types";

// ============================================================================
// Shared - Observable Reactive Programming
// ============================================================================
export {
  Observable,
  Observer,
  Subscription as ObservableSubscription,
  ObservableImpl,
  create,
} from "./shared/utils/observable.js";
export type { OperatorFunction } from "./shared/utils/observable.js";

// ============================================================================
// Shared - Type Definitions
// ============================================================================
export type { ThreadOptions, SDKOptions, SDKDependencies } from "./shared/types/core-types.js";

export type {
  ScriptFilter,
  ScriptOptions,
  ScriptTestResult,
  ScriptExecutionLog,
  ScriptStatistics,
  ScriptRegistrationConfig,
  ScriptBatchExecutionConfig,
} from "./shared/types/code-types.js";

export type {
  ExecutionEvent,
  StartEvent,
  CompleteEvent,
  ErrorEvent,
  CancelledEvent,
  ProgressEvent,
  NodeExecutedEvent,
} from "./shared/types/execution-events.js";

// ============================================================================
// Shared - Configuration Parsing Module
// ============================================================================
export {
  ConfigParser,
  ConfigTransformer,
  ConfigFormat,
  type ParsedConfig,
  type WorkflowConfigFile,
  type NodeConfigFile,
  type EdgeConfigFile,
  type IConfigParser,
  type IConfigTransformer,
} from "./shared/config/index.js";

// JSON Parsing Functions
export { parseJson, stringifyJson, validateJsonSyntax } from "./shared/config/index.js";

// TOML parsing function
export { parseToml, validateTomlSyntax } from "./shared/config/index.js";

// Configure the parser function
export {
  parseWorkflow,
  parseNodeTemplate,
  parseTriggerTemplate,
  parseScript,
  parseLLMProfile,
  loadConfigContent,
  parseAgentLoopConfig,
  parseAndValidateAgentLoopConfig,
  transformToAgentLoopConfig,
  loadAgentLoopConfig,
} from "./shared/config/index.js";

// ============================================================================
// Profile template type
// ============================================================================
export type { LLMProfileTemplate as ProfileTemplate } from "./shared/resources/llm/llm-profile-registry-api.js";

// ============================================================================
// Predefined resources - tools, workflows, triggers
// ============================================================================
export {
  createPredefinedTools,
  registerPredefinedTools,
  type PredefinedToolsOptions,
  type ReadFileConfig,
  type WriteFileConfig,
  type EditFileConfig,
  type RunShellConfig,
  type SessionNoteConfig,
} from "../resources/predefined/tools/index.js";

// ============================================================================
// Component Message System
// ============================================================================
export {
  MessageBus,
  createMessageBus,
  MessagePublisher,
  createMessagePublisher,
  matchesRoutingRule,
  findMatchingRule,
  sortRulesByPriority,
  type MessageFilter as ComponentMessageFilter,
  type MessageHandler,
  type MessageSubscription,
  type MessageBusOptions,
  type EntityStatus,
  type EntityContext,
} from "./shared/component-message/index.js";
