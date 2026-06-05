/**
 * Service Identifier
 * Defines the Symbol identifiers for all SDK services, providing type-safe service identifiers.
 *
 * Design Principles:
 * - Use Symbols as identifiers to avoid string spelling errors.
 * - Provide type-safe service access with proper TypeScript types.
 * - Organize identifiers by service hierarchy.
 */

import type { ServiceIdentifier } from "@wf-agent/common-utils";

// Import actual service types for type safety (using aliases to avoid naming conflicts)
import type { WorkflowGraphRegistry as WorkflowGraphRegistryType } from "../../workflow/stores/workflow-graph-registry.js";
import type { WorkflowExecutionRegistry as WorkflowExecutionRegistryType } from "../../workflow/stores/workflow-execution-registry.js";
import type { EventRegistry as EventRegistryType } from "../registry/event-registry.js";
import type { ToolRegistry as ToolRegistryType } from "../registry/tool-registry.js";
import type { ScriptRegistry as ScriptRegistryType } from "../registry/script-registry.js";
import type { WorkflowRegistry as WorkflowRegistryType } from "../../workflow/stores/workflow-registry.js";
import type { WorkflowRelationshipRegistry as WorkflowRelationshipRegistryType } from "../../workflow/stores/workflow-relationship-registry.js";
import type { NodeTemplateRegistry as NodeTemplateRegistryType } from "../registry/node-template-registry.js";
import type { TriggerTemplateRegistry as TriggerTemplateRegistryType } from "../registry/trigger-template-registry.js";
import type { TaskRegistry as TaskRegistryType } from "../../workflow/stores/task/task-registry.js";
import type { GlobalContext as GlobalContextType } from "../global-context.js";
import type { WorkflowExecutionBuilder as WorkflowExecutionBuilderType } from "../../workflow/execution/factories/workflow-execution-builder.js";
import type { WorkflowExecutor as WorkflowExecutorType } from "../../workflow/execution/executors/workflow-executor.js";
import type { WorkflowLifecycleCoordinator as WorkflowLifecycleCoordinatorType } from "../../workflow/execution/coordinators/workflow-lifecycle-coordinator.js";
import type { WorkflowStateTransitor as WorkflowStateTransitorType } from "../../workflow/execution/coordinators/workflow-state-transitor.js";
import type { CheckpointState as CheckpointStateType } from "../../workflow/checkpoint/checkpoint-state-manager.js";
import type { ConversationSession as GraphConversationSessionType } from "../messaging/conversation-session.js";
import type { LLMExecutor as LLMExecutorType } from "../executors/llm-executor.js";
import type { ToolCallExecutor as ToolCallExecutorType } from "../executors/tool-call-executor.js";
import type { ToolApprovalCoordinator as ToolApprovalCoordinatorType } from "../coordinators/tool-approval-coordinator.js";
import type { WorkflowExecutionCoordinator as WorkflowExecutionCoordinatorType } from "../../workflow/execution/coordinators/workflow-execution-coordinator.js";
import type { VariableCoordinator as VariableCoordinatorType } from "../../workflow/execution/coordinators/variable-coordinator.js";
import type { TriggerCoordinator as TriggerCoordinatorType } from "../../workflow/execution/coordinators/trigger-coordinator.js";
import type { NodeExecutionCoordinator as NodeExecutionCoordinatorType } from "../../workflow/execution/coordinators/node-execution-coordinator.js";
import type { LLMExecutionCoordinator as LLMExecutionCoordinatorType } from "../../workflow/execution/coordinators/llm-execution-coordinator.js";
import type { CheckpointCoordinator as CheckpointCoordinatorType } from "../../workflow/checkpoint/checkpoint-coordinator.js";
import type { ConversationSession as ConversationSessionType } from "../messaging/conversation-session.js";
import type { TriggerState as TriggerStateType } from "../../workflow/state-managers/trigger-state.js";
import type { InterruptionState as InterruptionStateType } from "../utils/interruption/interruption-state.js";
import type { SDKInstance as SDKInstanceType } from "../../api/shared/core/sdk-instance.js";
import type { TriggeredSubworkflowHandler as TriggeredSubworkflowHandlerType } from "../../workflow/execution/handlers/triggered-subworkflow-handler.js";
import type { WorkflowExecutionPool as WorkflowExecutionPoolType } from "../../workflow/execution/workflow-execution-pool.js";
import type { LLMWrapper as LLMWrapperType } from "../llm/wrapper.js";
import type { AgentLoopRegistry as AgentLoopRegistryType } from "../../agent/stores/agent-loop-registry.js";
import type { IAgentExecutionRegistry as AgentExecutionRegistryType } from "../../agent/stores/agent-execution-registry.js";
import type { ExecutionHierarchyRegistry as ExecutionHierarchyRegistryType } from "../registry/execution-hierarchy-registry.js";
import type { AgentLoopExecutor as AgentLoopExecutorType } from "../../agent/execution/executors/agent-loop-executor.js";
import type { AgentLoopCoordinator as AgentLoopCoordinatorType } from "../../agent/execution/coordinators/agent-loop-coordinator.js";
import type { SkillRegistry as SkillRegistryType } from "../registry/skill-registry.js";
import type { AgentProfileRegistry as AgentProfileRegistryType } from "../registry/agent-profile-registry.js";
import type { TimeoutRegistry as TimeoutRegistryType } from "../registry/timeout-registry.js";
import type {
  CheckpointStorageAdapter as CheckpointStorageAdapterType,
  WorkflowStorageAdapter as WorkflowStorageAdapterType,
  TaskStorageAdapter as TaskStorageAdapterType,
  WorkflowExecutionStorageAdapter as WorkflowExecutionStorageAdapterType,
  AgentLoopStorageAdapter as AgentLoopStorageAdapterType,
  FileCheckpointStorageAdapter as FileCheckpointStorageAdapterType,
} from "@wf-agent/storage";
import type { FileCheckpointManager as FileCheckpointManagerType } from "@wf-agent/common-utils";
import type { MetricsRegistry as MetricsRegistryType } from "../metrics/metrics-registry.js";
import type { ToolPermissionManager as ToolPermissionManagerType } from "../coordinators/tool-permission-manager.js";
import type { RejectionMessageBuilder as RejectionMessageBuilderType } from "../coordinators/rejection-message-builder.js";

// ============================================================
// Storage Layer Service
// ============================================================

/**
 * WorkflowGraphRegistry - Graph Registry
 * Manages the storage and retrieval of preprocessed graphs.
 */
export const WorkflowGraphRegistry: ServiceIdentifier<WorkflowGraphRegistryType> = Symbol("WorkflowGraphRegistry");

/**
 * WorkflowExecutionRegistry - Execution Registry
 * Manages the memory storage of WorkflowExecutionContext
 */
export const WorkflowExecutionRegistry: ServiceIdentifier<WorkflowExecutionRegistryType> = Symbol("WorkflowExecutionRegistry");

// ============================================================
// Business Layer Services
// ============================================================

/**
 * EventRegistry - Event Manager
 * Manages the publication and subscription to global events
 */
export const EventRegistry: ServiceIdentifier<EventRegistryType> = Symbol("EventRegistry");

/**
 * ToolRegistry - Tool Service
 * Manages the registration and execution of tools.
 */
export const ToolRegistry: ServiceIdentifier<ToolRegistryType> = Symbol("ToolRegistry");

/**
 * ScriptRegistry - Script Service
 * Manages the registration and execution of scripts
 */
export const ScriptRegistry: ServiceIdentifier<ScriptRegistryType> = Symbol("ScriptRegistry");

/**
 * WorkflowRegistry - Workflow Registry
 * Manages the entire lifecycle of workflow definitions, including reference management.
 */
export const WorkflowRegistry: ServiceIdentifier<WorkflowRegistryType> = Symbol("WorkflowRegistry");

/**
 * WorkflowRelationshipRegistry - Workflow Relationship Registry
 * Manages hierarchical relationships and reference relationships between workflows.
 */
export const WorkflowRelationshipRegistry: ServiceIdentifier<WorkflowRelationshipRegistryType> = Symbol("WorkflowRelationshipRegistry");

/**
 * NodeTemplateRegistry - Node Template Registry
 * Manages the registration and querying of node templates.
 */
export const NodeTemplateRegistry: ServiceIdentifier<NodeTemplateRegistryType> = Symbol("NodeTemplateRegistry");

/**
 * TriggerTemplateRegistry - Trigger Template Registry
 * Manages the registration and querying of trigger templates
 */
export const TriggerTemplateRegistry: ServiceIdentifier<TriggerTemplateRegistryType> =
  Symbol("TriggerTemplateRegistry");

/**
 * TaskRegistry - Task Registry
 * Manages the registration and querying of tasks
 */
export const TaskRegistry: ServiceIdentifier<TaskRegistryType> = Symbol("TaskRegistry");

// ============================================================
// Execution Layer Services
// ============================================================

/**
 * GlobalContext - Global Context
 * Provides access to registries and executors across the SDK instance
 */
export const GlobalContext: ServiceIdentifier<GlobalContextType> = Symbol("GlobalContext");

/**
 * WorkflowExecutionBuilder - Workflow Execution Builder
 * Constructs WorkflowExecutionEntity instances from workflow definitions
 */
export const WorkflowExecutionBuilder: ServiceIdentifier<WorkflowExecutionBuilderType> = Symbol("WorkflowExecutionBuilder");

/**
 * WorkflowExecutor - Workflow Executor
 * Executes a single WorkflowExecutionEntity instance
 */
export const WorkflowExecutor: ServiceIdentifier<WorkflowExecutorType> = Symbol("WorkflowExecutor");

/**
 * WorkflowLifecycleCoordinator - Workflow Lifecycle Coordinator
 * Manages the entire lifecycle of workflow executions
 */
export const WorkflowLifecycleCoordinator: ServiceIdentifier<WorkflowLifecycleCoordinatorType> = Symbol(
  "WorkflowLifecycleCoordinator",
);

/**
 * WorkflowStateTransitor - Workflow Execution State Transitor
 * Manages the transition of workflow execution lifecycle states
 */
export const WorkflowStateTransitor: ServiceIdentifier<WorkflowStateTransitorType> = Symbol("WorkflowStateTransitor");

/**
 * CheckpointState - Checkpoint State Manager
 * Manages the status of checkpoints
 */
export const CheckpointState: ServiceIdentifier<CheckpointStateType> = Symbol("CheckpointState");

/**
 * GraphConversationSession - Graph Conversation Session
 * Manages the conversation session for Graph workflows, with execution isolation.
 */
export const GraphConversationSession: ServiceIdentifier<GraphConversationSessionType> = Symbol(
  "GraphConversationSession",
);

/**
 * LLMExecutor - LLM Executor
 * Executes LLM nodes
 */
export const LLMExecutor: ServiceIdentifier<LLMExecutorType> = Symbol("LLMExecutor");

/**
 * ToolCallExecutor - A tool for executing tool calls, specifically designed to handle the execution of tool calls.
 *
 */
export const ToolCallExecutor: ServiceIdentifier<ToolCallExecutorType> = Symbol("ToolCallExecutor");

/**
 * ToolApprovalCoordinator - Tool Approval Coordinator
 * Coordinates the tool approval process
 */
export const ToolApprovalCoordinator: ServiceIdentifier<ToolApprovalCoordinatorType> =
  Symbol("ToolApprovalCoordinator");

// ============================================================
// Execution Layer - Coordinators
// ============================================================

/**
 * WorkflowExecutionCoordinator - Workflow Execution Coordinator
 * Coordinates the execution process of workflow executions, orchestrating the completion of tasks by various components.
 */
export const WorkflowExecutionCoordinator: ServiceIdentifier<WorkflowExecutionCoordinatorType> = Symbol(
  "WorkflowExecutionCoordinator",
);

/**
 * VariableCoordinator - Variable Coordinator
 * Responsible for the coordination logic of variables, including validation, initialization on demand, event triggering, and more.
 */
export const VariableCoordinator: ServiceIdentifier<VariableCoordinatorType> = Symbol("VariableCoordinator");

/**
 * TriggerCoordinator - Trigger Coordinator
 * Responsible for the registration, deregistration, and execution of trigger actions.
 */
export const TriggerCoordinator: ServiceIdentifier<TriggerCoordinatorType> = Symbol("TriggerCoordinator");

/**
 * NodeExecutionCoordinator - Node Execution Coordinator
 * Responsible for coordinating the execution process of nodes, including event triggering, Hook execution, and subgraph processing.
 */
export const NodeExecutionCoordinator: ServiceIdentifier<NodeExecutionCoordinatorType> = Symbol(
  "NodeExecutionCoordinator",
);

/**
 * LLMExecutionCoordinator - LLM Execution Coordinator
 * Responsible for coordinating the entire process of LLM (Large Language Model) calls and tool invocations.
 */
export const LLMExecutionCoordinator: ServiceIdentifier<LLMExecutionCoordinatorType> =
  Symbol("LLMExecutionCoordinator");

/**
 * CheckpointCoordinator - The checkpoint coordinator
 * Coordinates the entire checkpoint process
 */
export const CheckpointCoordinator: ServiceIdentifier<CheckpointCoordinatorType> = Symbol("CheckpointCoordinator");

// ============================================================
// Execution Layer - Managers
// ============================================================

/**
 * ConversationSession - Conversation Manager
 * Manages the message history and message indexing.
 */
export const ConversationSession: ServiceIdentifier<ConversationSessionType> = Symbol("ConversationSession");

/**
 * VariableManager - Unified Variable State Manager
 * Manages variable definitions, values, and scope stacks in a single Map structure
 */
export const VariableManager: ServiceIdentifier<import("../../workflow/state-managers/variable-manager.js").VariableManager> = Symbol("VariableManager");

/**
 * TriggerState - Trigger State Manager
 * Specifically manages the runtime state of triggers
 */
export const TriggerState: ServiceIdentifier<TriggerStateType> = Symbol("TriggerState");

/**
 * InterruptionState - Interrupt Manager
 * Manages the workflow execution interruption status and operations in a unified manner
 */
export const InterruptionState: ServiceIdentifier<InterruptionStateType> = Symbol("InterruptionState");

// ============================================================
// API Layer Service
// ============================================================

/**
 * SDK - Main Class of the SDK
 * Provides a unified API entry point
 */
export const SDK: ServiceIdentifier<SDKInstanceType> = Symbol("SDK");

/**
 * SDKOptions - SDK Configuration Options
 * Stores the configuration options passed during SDK initialization
 */
export const SDKOptions: ServiceIdentifier<import("../../api/shared/types/core-types.js").SDKOptions> = Symbol("SDKOptions");

/**
 * TriggeredSubworkflowHandler - Manages the execution of triggered sub-workflows
 *
 */
export const TriggeredSubworkflowHandler: ServiceIdentifier<TriggeredSubworkflowHandlerType> = Symbol(
  "TriggeredSubworkflowHandler",
);

/**
 * WorkflowExecutionPool - Workflow Execution Pool Service
 * Manages a pool of WorkflowExecutor instances, providing global management of workflow execution pool resources.
 */
export const WorkflowExecutionPool: ServiceIdentifier<WorkflowExecutionPoolType> = Symbol("WorkflowExecutionPool");

// ============================================================
// Core Layer Services
// ============================================================

/**
 * LLMWrapper - LLM Wrapper
 * Provides a unified interface for calling LLMs (Large Language Models)
 */
export const LLMWrapper: ServiceIdentifier<LLMWrapperType> = Symbol("LLMWrapper");

// ============================================================
// Agent Layer Service
// ============================================================

/**
 * AgentLoopRegistry - Agent Loop Registry
 * Manages the memory storage of AgentLoopEntities
 */
export const AgentLoopRegistry: ServiceIdentifier<AgentLoopRegistryType> = Symbol("AgentLoopRegistry");

/**
 * AgentExecutionRegistry - Agent Execution Registry Interface
 * Provides unified access to agent loop execution instances
 */
export const AgentExecutionRegistry: ServiceIdentifier<AgentExecutionRegistryType> = Symbol("AgentExecutionRegistry");

/**
 * ExecutionHierarchyRegistry - Unified Execution Hierarchy Registry
 * Manages parent-child relationships across all execution types (Workflow, Agent)
 * Provides unified cleanup and query operations for mixed hierarchies
 */
export const ExecutionHierarchyRegistry: ServiceIdentifier<ExecutionHierarchyRegistryType> = Symbol("ExecutionHierarchyRegistry");

/**
 * AgentLoopExecutor - Agent Loop Executor
 * Responsible for executing the iterative loop of the Agent tool, creating an independent message history with each execution.
 */
export const AgentLoopExecutor: ServiceIdentifier<AgentLoopExecutorType> = Symbol("AgentLoopExecutor");

/**
 * AgentLoopCoordinator - Agent Loop Lifecycle Coordinator
 * Coordinates the full lifecycle management of AgentLoopEntities
 */
export const AgentLoopCoordinator: ServiceIdentifier<AgentLoopCoordinatorType> = Symbol("AgentLoopCoordinator");

// ============================================================
// Skill Layer Service
// ============================================================

/**
 * SkillRegistry - Skill Registry
 * Manages the discovery, parsing, and querying of Skills
 */
export const SkillRegistry: ServiceIdentifier<SkillRegistryType> = Symbol("SkillRegistry");

/**
 * AgentProfileRegistry - Agent Profile Registry
 * Manages the registration and discovery of agent profile metadata.
 * Provides a single source of truth for available agent profiles.
 */
export const AgentProfileRegistry: ServiceIdentifier<AgentProfileRegistryType> = Symbol("AgentProfileRegistry");

/**
 * TimeoutRegistry - Timeout Registry
 * Manages TimeoutManager instances across all executions with centralized timeout operations.
 */
export const TimeoutRegistry: ServiceIdentifier<TimeoutRegistryType> = Symbol("TimeoutRegistry");

// ============================================================
// Storage Adapter Services
// ============================================================

/**
 * CheckpointStorageAdapter - Checkpoint Storage Adapter
 * Provides checkpoint persistence operations
 */
export const CheckpointStorageAdapter: ServiceIdentifier<CheckpointStorageAdapterType> = Symbol("CheckpointStorageAdapter");

/**
 * WorkflowStorageAdapter - Workflow Storage Adapter
 * Provides workflow definition persistence operations
 */
export const WorkflowStorageAdapter: ServiceIdentifier<WorkflowStorageAdapterType> = Symbol("WorkflowStorageAdapter");

/**
 * TaskStorageAdapter - Task Storage Adapter
 * Provides task persistence operations
 */
export const TaskStorageAdapter: ServiceIdentifier<TaskStorageAdapterType> = Symbol("TaskStorageAdapter");

/**
 * WorkflowExecutionStorageAdapter - Workflow Execution Storage Adapter
 * Provides workflow execution history persistence operations
 */
export const WorkflowExecutionStorageAdapter: ServiceIdentifier<WorkflowExecutionStorageAdapterType> = Symbol("WorkflowExecutionStorageAdapter");

/**
 * AgentLoopStorageAdapter - Agent Loop Storage Adapter
 * Provides agent loop lifecycle persistence operations
 */
export const AgentLoopStorageAdapter: ServiceIdentifier<AgentLoopStorageAdapterType> = Symbol("AgentLoopStorageAdapter");

/**
 * MetricsStorageAdapter - Metrics Storage Adapter
 * Provides metrics persistence operations
 */
export const MetricsStorageAdapter: ServiceIdentifier<import("@wf-agent/storage").MetricsStorageAdapter> = Symbol("MetricsStorageAdapter");

/**
 * FileCheckpointManager - File Checkpoint Manager
 * Manages workspace file state checkpoints independent of VFS
 */
export const FileCheckpointManager: ServiceIdentifier<FileCheckpointManagerType> = Symbol("FileCheckpointManager");

/**
 * FileCheckpointStorageAdapter - File Checkpoint Storage Adapter
 * Storage adapter for file checkpoint persistence
 */
export const FileCheckpointStorageAdapter: ServiceIdentifier<FileCheckpointStorageAdapterType> = Symbol("FileCheckpointStorageAdapter");

// ============================================================
// Metrics Services
// ============================================================

/**
 * MetricsRegistry - Metrics Registry
 * Manages all metrics collectors and provides centralized access
 */
export const MetricsRegistry: ServiceIdentifier<MetricsRegistryType> = Symbol("MetricsRegistry");

/**
 * MetricsConfig - Metrics Configuration
 * Provides configuration for the metrics system
 */
export const MetricsConfig: ServiceIdentifier<import("@wf-agent/types").MetricsConfig> = Symbol("MetricsConfig");

/**
 * TimeoutConfig - Timeout Configuration
 * Provides configuration for timeout values across SDK operations
 */
export const TimeoutConfig: ServiceIdentifier<Required<import("@wf-agent/types").TimeoutConfig>> = Symbol("TimeoutConfig");

// ============================================================
// Tool Permission Services (New Architecture)
// ============================================================

/**
 * ToolPermissionManager - Tool Permission Manager
 * Manages runtime tool permissions (enabled/disabled state)
 */
export const ToolPermissionManager: ServiceIdentifier<ToolPermissionManagerType> = Symbol("ToolPermissionManager");

/**
 * RejectionMessageBuilder - Rejection Message Builder
 * Builds customized rejection messages for blocked tools
 */
export const RejectionMessageBuilder: ServiceIdentifier<RejectionMessageBuilderType> = Symbol("RejectionMessageBuilder");
