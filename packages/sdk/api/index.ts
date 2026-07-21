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
  ExecutionCommand,
  ManagementCommand,
  QueryCommand,
  StreamingCommand,
  SyncCommand,
  BaseSyncCommand,
  CommandMetadata,
  CommandMetadataDefinition,
  CommandValidationResult,
  validationSuccess,
  validationFailure,
} from "./shared/types/command.js";

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
  OnEventSubscription,
  OnceEventSubscription,
  WaitForEventSubscription,
  createExecutionScopedSubscription,
  createExecutionScopedOnceSubscription,
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

// Two-Layer Architecture SDK Exports
export { createSDK } from "./shared/core/sdk.js";

// SDK Instance Type
export type { SDKInstance } from "./shared/core/sdk-instance.js";

// Global Context exports (for advanced use cases)
export type { GlobalContext } from "../shared/global-context.js";

// SDK Types
export type {
  SDKOptions,
  SDKLifecycleHooks,
  GracefulShutdownConfig,
} from "./shared/types/core-types.js";

// API Factory
export { APIFactory, type AllAPIs } from "./shared/core/api-factory.js";

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
  type ExecutionTimeline,
  type ExecutionTimelinePhase,
  type ExecutionTimelineSummary,
} from "./shared/resources/events/event-resource-api.js";

// Common Resource API Base Classes and Tools
export {
  QueryableResourceAPI,
  SimplifiedCrudResourceAPI,
  type WritableResourceAPI,
  type ClearableResourceAPI,
} from "./shared/resources/generic-resource-api.js";
export { createSharedResourceAPIs, type SharedResourceAPIs } from "./shared/resources/index.js";

// Metrics Resource API
export {
  MetricsResourceAPI,
  type WorkflowMetricsQuery,
  type NodeMetricsQuery,
  type AgentMetricsQuery,
  type MetricsExportFormat,
} from "./shared/resources/metrics/metrics-resource-api.js";

// Task Resource API
export {
  TaskResourceAPI,
  type TaskFilter,
  type TaskSummary,
  type TaskStats,
} from "./shared/resources/tasks/task-resource-api.js";

// Workflow Graph Query API
export {
  WorkflowGraphQueryAPI,
  type WorkflowGraphSummary,
  type GraphNodeStats,
  type GraphEdgeStats,
  type NodeNeighbors,
} from "./shared/resources/graphs/workflow-graph-query-api.js";

// Storage Diagnostics API
export {
  StorageDiagnosticsAPI,
  type StorageAdapterHealth,
  type StorageItemCounts,
  type StorageDiagnosticsReport,
} from "./shared/resources/diagnostics/storage-diagnostics-api.js";

// Search API
export {
  SearchAPI,
  type SearchResourceType,
  type SearchOptions,
  type SearchResultItem,
  type SearchResult,
} from "./shared/resources/search/search-api.js";

// ============================================================================
// Workflow - Resource Management API (CRUD Operations)
// ============================================================================
export { WorkflowRegistryAPI } from "./workflow/resources/workflow-registry-api.js";
export {
  WorkflowExecutionRegistryAPI,
  type WorkflowExecutionFilter,
  type WorkflowExecutionSummary,
} from "./workflow/resources/workflow-execution-registry-api.js";
export { NodeRegistryAPI } from "./workflow/resources/node-template-registry-api.js";
export { TriggerTemplateRegistryAPI } from "./workflow/resources/trigger-template-registry-api.js";
export {
  CheckpointResourceAPI,
  type CheckpointFilter,
  type CheckpointSummary,
  type CheckpointTransition,
  type CheckpointChainAnalysis,
} from "./workflow/resources/checkpoint-resource-api.js";
export {
  FileCheckpointResourceAPI,
  type FileCheckpointFilter,
} from "./workflow/resources/file-checkpoint-resource-api.js";
export {
  MessageResourceAPI,
  type MessageFilter as WorkflowMessageFilter,
  type MessageStats,
} from "./workflow/resources/message-resource-api.js";
export {
  VariableResourceAPI,
  type VariableDefinition,
  type VariableFilter,
} from "./workflow/resources/variable-resource-api.js";
export { TriggerResourceAPI } from "./workflow/resources/trigger-resource-api.js";
export {
  UserInteractionResourceAPI,
  type UserInteractionConfig,
  type UserInteractionFilter,
} from "./workflow/resources/user-interaction-resource-api.js";
export {
  WorkflowExecutionGraphQueryAPI,
  type WorkflowExecutionNode,
  type WorkflowExecutionEdge,
  type WorkflowExecutionGraph,
  type WorkflowExecutionPathSummary,
  type WorkflowExecutionGraphFilter,
} from "./workflow/resources/workflow-execution-graph-query-api.js";

// ============================================================================
// Commands - Unified Command Interface
// ============================================================================

// Shared Commands
export { GenerateCommand, type GenerateParams } from "./shared/operations/generate-command.js";
export { GenerateBatchCommand, type GenerateBatchParams } from "./shared/operations/generate-batch-command.js";
export { ExecuteToolCommand, type ExecuteToolParams } from "./shared/operations/tools/execute-tool-command.js";
export { ExecuteScriptCommand, type ExecuteScriptParams } from "./shared/operations/scripts/execute-script-command.js";
export { DispatchEventCommand, type DispatchEventParams } from "./shared/operations/events/dispatch-event-command.js";

// Workflow Commands
export { ExecuteWorkflowCommand, type ExecuteWorkflowParams } from "./workflow/operations/execution/execute-workflow-command.js";
export { ExecuteWorkflowStreamCommand, type ExecuteWorkflowStreamParams } from "./workflow/operations/execution/execute-workflow-stream-command.js";
export { PauseWorkflowCommand, type PauseWorkflowParams } from "./workflow/operations/execution/pause-workflow-command.js";
export { ResumeWorkflowCommand, type ResumeWorkflowParams } from "./workflow/operations/execution/resume-workflow-command.js";
export { CancelWorkflowCommand, type CancelWorkflowParams } from "./workflow/operations/execution/cancel-workflow-command.js";
export { CreateCheckpointCommand as WorkflowCreateCheckpointCommand, type CreateCheckpointParams as WorkflowCreateCheckpointParams } from "./workflow/operations/checkpoints/create-checkpoint-command.js";
export { RestoreFromCheckpointCommand, type RestoreFromCheckpointParams } from "./workflow/operations/checkpoints/restore-from-checkpoint-command.js";
export { EnableTriggerCommand, type EnableTriggerParams } from "./workflow/operations/triggers/enable-trigger-command.js";
export { DisableTriggerCommand, type DisableTriggerParams } from "./workflow/operations/triggers/disable-trigger-command.js";

// Agent Commands
export { RunAgentLoopCommand, type RunAgentLoopParams } from "./agent/operations/run-agent-loop-command.js";
export { RunAgentLoopStreamCommand, type RunAgentLoopStreamParams } from "./agent/operations/run-agent-loop-stream-command.js";
export { CancelAgentLoopCommand, type CancelAgentLoopParams } from "./agent/operations/cancel-agent-loop-command.js";
export { PauseAgentLoopCommand, type PauseAgentLoopParams } from "./agent/operations/pause-agent-loop-command.js";
export { ResumeAgentLoopCommand, type ResumeAgentLoopParams } from "./agent/operations/resume-agent-loop-command.js";
export { CreateCheckpointCommand as AgentCreateCheckpointCommand, type CreateCheckpointParams as AgentCreateCheckpointParams } from "./agent/operations/checkpoints/create-checkpoint-command.js";
export { RestoreCheckpointCommand as AgentRestoreCheckpointCommand, type RestoreCheckpointParams as AgentRestoreCheckpointParams } from "./agent/operations/checkpoints/restore-checkpoint-command.js";
export { EnableAgentTriggerCommand, type EnableAgentTriggerParams } from "./agent/operations/triggers/enable-agent-trigger-command.js";
export { DisableAgentTriggerCommand, type DisableAgentTriggerParams } from "./agent/operations/triggers/disable-agent-trigger-command.js";

// Command Validators
export {
  validateGenerateParams,
  validateToolExecutionParams,
  validateScriptExecutionParams,
  validateEventDispatchParams,
} from "./shared/operations/validators/shared-validators.js";
export {
  validateWorkflowExecutionParams,
  validateWorkflowLifecycleParams,
  validateCheckpointCreationParams,
  validateCheckpointRestorationParams,
  validateTriggerParams,
} from "./shared/operations/validators/workflow-validators.js";
export {
  validateAgentLoopRunParams,
  validateAgentLoopControlParams,
  validateAgentCheckpointCreationParams,
  validateAgentCheckpointRestorationParams,
} from "./shared/operations/validators/agent-validators.js";

// ============================================================================
// Workflow - Builder
// ============================================================================
export { WorkflowBuilder, ExecutionBuilder } from "./workflow/builders/index.js";
export { NodeTemplateBuilder } from "./workflow/builders/node-template-builder.js";
export { TriggerTemplateBuilder } from "./workflow/builders/trigger-template-builder.js";

// ============================================================================
// Agent - Builders
// ============================================================================
export {
  AgentLoopConfigBuilder,
  AgentDefinitionBuilder,
  AgentToolConfigBuilder,
  AgentHookBuilder,
  AgentTriggerBuilder,
} from "./agent/builders/index.js";

// ============================================================================
// Workflow - Validation API
// ============================================================================
export { WorkflowValidator as WorkflowValidatorAPI } from "../workflow/validation/index.js";
export { CodeConfigValidator as CodeConfigValidatorAPI } from "../workflow/validation/script-config-validator.js";
export { StaticValidator as StaticValidatorAPI } from "../shared/validation/index.js";
export { StaticValidator } from "../shared/validation/index.js";
export { RuntimeValidator as RuntimeValidatorAPI } from "../shared/validation/index.js";
export {
  validateHook as validateHookAPI,
  validateHooks as validateHooksAPI,
} from "../shared/validation/index.js";
export {
  validateTriggerCondition as validateTriggerConditionAPI,
  validateExecuteTriggeredSubworkflowActionConfig as validateExecuteTriggeredSubworkflowActionConfigAPI,
  validateTriggerAction as validateTriggerActionAPI,
  validateWorkflowTrigger as validateWorkflowTriggerAPI,
  validateTriggerReference as validateTriggerReferenceAPI,
  validateTriggers as validateTriggersAPI,
} from "../shared/validation/index.js";

// ============================================================================
// Agent - Resources
// ============================================================================
export {
  AgentLoopRegistryAPI,
  type AgentLoopFilter,
  type AgentLoopSummary,
  type IterationDetail,
  type IterationHistorySummary,
} from "./agent/resources/agent-loop-registry-api.js";

export {
  AgentExecutionRegistryAPI,
  type AgentExecutionFilter,
  type AgentExecutionSummary,
} from "./agent/resources/agent-execution-registry-api.js";

export {
  /** @deprecated Use {@link AgentLoopRegistryAPI} instead */
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
  type OptimizationOpportunity,
} from "./agent/resources/agent-loop-iteration-api.js";

export {
  AgentVariableResourceAPI,
  type AgentVariableFilter,
  type AgentVariableUpdateOptions,
  type AgentVariableDefinition,
  type AgentContextVariable,
} from "./agent/resources/agent-variable-resource-api.js";

export {
  AgentUserInteractionResourceAPI,
  type AgentUserInteractionConfig,
  type AgentUserInteractionFilter,
  type AgentUserInteractionEventRecord,
} from "./agent/resources/agent-user-interaction-resource-api.js";

export {
  AgentTriggerResourceAPI,
  type AgentTriggerFilter,
} from "./agent/resources/agent-trigger-resource-api.js";

export {
  AgentFileCheckpointResourceAPI,
  type AgentFileCheckpointFilter,
} from "./agent/resources/agent-file-checkpoint-resource-api.js";

export {
  AgentTriggerTemplateRegistryAPI,
  type AgentTriggerTemplateFilter,
  type AgentTriggerTemplateSummary,
} from "./agent/resources/agent-trigger-template-registry-api.js";

export {
  AgentHookTemplateRegistryAPI,
  type AgentHookTemplateFilter,
  type AgentHookTemplateSummary,
} from "./agent/resources/agent-hook-template-registry-api.js";

export {
  AgentTemplateRegistryAPI,
  type AgentTemplateFilter,
  type AgentTemplateSummary,
} from "./agent/resources/agent-template-registry-api.js";

export {
  AgentErrorAnalysisAPI,
  type RootCauseAnalysis,
  type ErrorStatistics,
  type ErrorRecoveryProposal,
} from "./agent/resources/errors/index.js";

// ============================================================================
// Shared - Types
// ============================================================================
export { CommandError, CommandValidationError, CommandNotFoundError, CommandTimeoutError } from "./shared/types/command-error.js";

// ============================================================================
// Shared - Utilities
// ============================================================================
export { Observable, create, type Observer } from "./shared/utils/observable.js";

// Validation utilities for Commands
export {
  validateRequiredString,
  validateRequiredId,
  validateOptionalPositiveInt,
  validateRequiredEntity,
  combineErrors,
} from "./shared/operations/validation-utils.js";

// ============================================================================
// Shared - Configuration Parsing Utilities
// ============================================================================
export {
  // Configuration types & utilities
  ConfigFormat,
  getConfigFormatFromPath,
  initializeTomlParser,
  parseToml,
  parseJson,
  // Accessor
  createConfigAccessor,
  createLazyConfigAccessor,
  createSingletonAccessor,
  createKeyAccessor,
  type ConfigAccessor,
  type ConfigKeyAccessor,
  // Environment mapping
  applyEnvOverrides,
  createEnvMapping,
  EnvParsers,
  EnvPrefixes,
  toEnvName,
  type EnvMapping,
  type EnvMappingEntry,
  type EnvParser,
  // Validator
  validateConfig,
  validateConfigOrThrow,
  validateConfigs,
  FieldValidator,
  createCompositeValidator,
  type ValidationResult,
  // Config index
  loadConfigIndex,
  loadMultipleConfigIndexes,
  registerResolver,
  hasResolver,
  listIndexTypes,
  type IndexType,
  type IndexResolver,
  type IndexEntryType,
  // Parse functions (pure data processing, no file I/O)
  parseWorkflow,
  parseNodeTemplate,
  parseTriggerTemplate,
  parseHookTemplate,
  parseScript,
  parseLLMProfile,
  parsePromptTemplateConfig,
  // Validate functions
  validateLLMProfile,
  validateNodeTemplate,
  validateTriggerTemplate,
  validateHookTemplate,
  validateScript,
  validatePromptTemplate,
  // Transforms
  transformToAgentLoopConfig,
  // Processors (pure: merge / validate / export)
  mergeMetricsWithDefaults,
  getMetricsEnvironmentDefaults,
  mergeTimeoutWithDefaults,
  getTimeoutEnvironmentDefaults,
  mergeFileCheckpointConfig,
  mergeStorageWithDefaults,
  getStorageEnvironmentDefaults,
  mergeOutputWithDefaults,
  getOutputEnvironmentDefaults,
  getPresetsEnvironmentDefaults,
  validatePresetsConfig,
  transformPresetsConfig,
  exportPresetsConfig,
  // Sandbox
  mergeSandboxWithDefaults,
  validateSandboxConfig,
  transformSandboxConfig,
  exportSandboxConfig,
  // Tool processors
  validateReadFileConfig,
  transformReadFileConfig,
  exportReadFileConfig,
} from "./shared/config/index.js";

// Configuration types (for application layer config loaders)
export type {
  ParsedAgentLoopConfig,
  AgentLoopConfigFile,
  ParsedLLMProfileConfig,
  ParsedNodeTemplateConfig,
  ParsedTriggerTemplateConfig,
  ParsedHookTemplateConfig,
  ParsedScriptConfig,
  ParsedPromptTemplateConfig,
  PromptTemplateConfigFile,
} from "./shared/config/index.js";

// ============================================================================
// Core - Coordinators
// ============================================================================
export { ToolApprovalCoordinator, LLMExecutionCoordinator } from "../shared/coordinators/index.js";
