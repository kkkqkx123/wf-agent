/**
 * Service Identifier
 * Defines the Symbol identifiers for all SDK services, providing type-safe service identifiers.
 *
 * Design Principles:
 * - Use Symbols as identifiers to avoid string spelling errors.
 * - Provide type-safe service access.
 * - Organize identifiers by service hierarchy.
 */

import type { ServiceIdentifier } from "@wf-agent/common-utils";

// Forward declarations to avoid circular dependencies
// These will be properly typed via module augmentation or by the container

// ============================================================
// Storage Layer Service
// ============================================================

/**
 * WorkflowGraphRegistry - Graph Registry
 * Manages the storage and retrieval of preprocessed graphs.
 */
export const WorkflowGraphRegistry: ServiceIdentifier<unknown> = Symbol("WorkflowGraphRegistry");

/**
 * GraphNavigator - Graph Navigator
 * Provides functionality for navigating and traversing graphs.
 */
export const GraphNavigator: ServiceIdentifier<unknown> = Symbol("GraphNavigator");

/**
 * WorkflowExecutionRegistry - Execution Registry
 * Manages the memory storage of WorkflowExecutionContext
 */
export const WorkflowExecutionRegistry: ServiceIdentifier<unknown> = Symbol("WorkflowExecutionRegistry");

// ============================================================
// Business Layer Services
// ============================================================

/**
 * EventRegistry - Event Manager
 * Manages the publication and subscription to global events
 */
export const EventRegistry: ServiceIdentifier<unknown> = Symbol("EventRegistry");

/**
 * ToolRegistry - Tool Service
 * Manages the registration and execution of tools.
 */
export const ToolRegistry: ServiceIdentifier<unknown> = Symbol("ToolRegistry");

/**
 * ScriptRegistry - Script Service
 * Manages the registration and execution of scripts
 */
export const ScriptRegistry: ServiceIdentifier<unknown> = Symbol("ScriptRegistry");

/**
 * WorkflowRegistry - Workflow Registry
 * Manages the entire lifecycle of workflow definitions, including reference management.
 */
export const WorkflowRegistry: ServiceIdentifier<unknown> = Symbol("WorkflowRegistry");

/**
 * NodeTemplateRegistry - Node Template Registry
 * Manages the registration and querying of node templates.
 */
export const NodeTemplateRegistry: ServiceIdentifier<unknown> = Symbol("NodeTemplateRegistry");

/**
 * TriggerTemplateRegistry - Trigger Template Registry
 * Manages the registration and querying of trigger templates
 */
export const TriggerTemplateRegistry: ServiceIdentifier<unknown> =
  Symbol("TriggerTemplateRegistry");

/**
 * TaskRegistry - Task Registry
 * Manages the registration and querying of tasks
 */
export const TaskRegistry: ServiceIdentifier<unknown> = Symbol("TaskRegistry");

// ============================================================
// Execution Layer Services
// ============================================================

/**
 * WorkflowExecutionBuilder - Workflow Execution Builder
 * Constructs WorkflowExecutionEntity instances from workflow definitions
 */
export const WorkflowExecutionBuilder: ServiceIdentifier<unknown> = Symbol("WorkflowExecutionBuilder");

/**
 * WorkflowExecutor - Workflow Executor
 * Executes a single WorkflowExecutionEntity instance
 */
export const WorkflowExecutor: ServiceIdentifier<unknown> = Symbol("WorkflowExecutor");

/**
 * WorkflowLifecycleCoordinator - Workflow Lifecycle Coordinator
 * Manages the entire lifecycle of workflow executions
 */
export const WorkflowLifecycleCoordinator: ServiceIdentifier<unknown> = Symbol(
  "WorkflowLifecycleCoordinator",
);

/**
 * WorkflowStateTransitor - Workflow Execution State Transitor
 * Manages the transition of workflow execution lifecycle states
 */
export const WorkflowStateTransitor: ServiceIdentifier<unknown> = Symbol("WorkflowStateTransitor");

/**
 * CheckpointState - Checkpoint State Manager
 * Manages the status of checkpoints
 */
export const CheckpointState: ServiceIdentifier<unknown> = Symbol("CheckpointState");

/**
 * ToolContextStore - Tool Context Store
 * Manages the execution context of tools
 */
export const ToolContextStore: ServiceIdentifier<unknown> = Symbol("ToolContextStore");

/**
 * GraphConversationSession - Graph Conversation Session
 * Manages the conversation session for Graph workflows, with execution isolation.
 */
export const GraphConversationSession: ServiceIdentifier<unknown> = Symbol(
  "GraphConversationSession",
);

/**
 * LLMExecutor - LLM Executor
 * Executes LLM nodes
 */
export const LLMExecutor: ServiceIdentifier<unknown> = Symbol("LLMExecutor");

/**
 * ToolCallExecutor - A tool for executing tool calls, specifically designed to handle the execution of tool calls.
 *
 */
export const ToolCallExecutor: ServiceIdentifier<unknown> = Symbol("ToolCallExecutor");

/**
 * ToolApprovalCoordinator - Tool Approval Coordinator
 * Coordinates the tool approval process
 */
export const ToolApprovalCoordinator: ServiceIdentifier<unknown> =
  Symbol("ToolApprovalCoordinator");

// ============================================================
// Execution Layer - Coordinators
// ============================================================

/**
 * WorkflowExecutionCoordinator - Workflow Execution Coordinator
 * Coordinates the execution process of workflow executions, orchestrating the completion of tasks by various components.
 */
export const WorkflowExecutionCoordinator: ServiceIdentifier<unknown> = Symbol(
  "WorkflowExecutionCoordinator",
);

/**
 * VariableCoordinator - Variable Coordinator
 * Responsible for the coordination logic of variables, including validation, initialization on demand, event triggering, and more.
 */
export const VariableCoordinator: ServiceIdentifier<unknown> = Symbol("VariableCoordinator");

/**
 * TriggerCoordinator - Trigger Coordinator
 * Responsible for the registration, deregistration, and execution of trigger actions.
 */
export const TriggerCoordinator: ServiceIdentifier<unknown> = Symbol("TriggerCoordinator");

/**
 * NodeExecutionCoordinator - Node Execution Coordinator
 * Responsible for coordinating the execution process of nodes, including event triggering, Hook execution, and subgraph processing.
 */
export const NodeExecutionCoordinator: ServiceIdentifier<unknown> = Symbol(
  "NodeExecutionCoordinator",
);

/**
 * LLMExecutionCoordinator - LLM Execution Coordinator
 * Responsible for coordinating the entire process of LLM (Large Language Model) calls and tool invocations.
 */
export const LLMExecutionCoordinator: ServiceIdentifier<unknown> =
  Symbol("LLMExecutionCoordinator");

/**
 * ToolVisibilityCoordinator - Tool Visibility Coordinator
 * Manages the runtime visibility of tools and generates visibility declaration messages.
 */
export const ToolVisibilityCoordinator: ServiceIdentifier<unknown> = Symbol(
  "ToolVisibilityCoordinator",
);

/**
 * ToolVisibilityStore - Tool Visibility Store
 * Manages the visibility status of tools
 */
export const ToolVisibilityStore: ServiceIdentifier<unknown> = Symbol("ToolVisibilityStore");

/**
 * WorkflowOperationCoordinator - Workflow Operation Coordinator
 * Responsible for coordinating the structural operations of workflow executions (Fork/Join/Copy)
 */
export const WorkflowOperationCoordinator: ServiceIdentifier<unknown> = Symbol(
  "WorkflowOperationCoordinator",
);

/**
 * CheckpointCoordinator - The checkpoint coordinator
 * Coordinates the entire checkpoint process
 */
export const CheckpointCoordinator: ServiceIdentifier<unknown> = Symbol("CheckpointCoordinator");

// ============================================================
// Execution Layer - Managers
// ============================================================

/**
 * ConversationSession - Conversation Manager
 * Manages the message history and message indexing.
 */
export const ConversationSession: ServiceIdentifier<unknown> = Symbol("ConversationSession");

/**
 * VariableState - Variable State Manager
 * Specializes in managing the runtime state of variables
 */
export const VariableState: ServiceIdentifier<unknown> = Symbol("VariableState");

/**
 * TriggerState - Trigger State Manager
 * Specifically manages the runtime state of triggers
 */
export const TriggerState: ServiceIdentifier<unknown> = Symbol("TriggerState");

/**
 * InterruptionState - Interrupt Manager
 * Manages the workflow execution interruption status and operations in a unified manner
 */
export const InterruptionState: ServiceIdentifier<unknown> = Symbol("InterruptionState");

// ============================================================
// API Layer Service
// ============================================================

/**
 * ExecutionContext - Provides the global execution context
 *
 */
export const ExecutionContext: ServiceIdentifier<unknown> = Symbol("ExecutionContext");

/**
 * APIDependencyManager - API Dependency Manager
 * Manages all Core layer dependencies required at the API level in a unified manner.
 */
export const APIDependencyManager: ServiceIdentifier<unknown> = Symbol("APIDependencyManager");

/**
 * APIFactory - The API Factory
 * Manages the creation of all resource API instances in a unified manner
 */
export const APIFactory: ServiceIdentifier<unknown> = Symbol("APIFactory");

/**
 * SDK - Main Class of the SDK
 * Provides a unified API entry point
 */
export const SDK: ServiceIdentifier<unknown> = Symbol("SDK");

/**
 * TriggeredSubworkflowHandler - Manages the execution of triggered sub-workflows
 *
 */
export const TriggeredSubworkflowHandler: ServiceIdentifier<unknown> = Symbol(
  "TriggeredSubworkflowHandler",
);

/**
 * WorkflowExecutionPool - Workflow Execution Pool Service
 * Manages a pool of WorkflowExecutor instances, providing global management of workflow execution pool resources.
 */
export const WorkflowExecutionPool: ServiceIdentifier<unknown> = Symbol("WorkflowExecutionPool");

// ============================================================
// Core Layer Services
// ============================================================

/**
 * LLMWrapper - LLM Wrapper
 * Provides a unified interface for calling LLMs (Large Language Models)
 */
export const LLMWrapper: ServiceIdentifier<unknown> = Symbol("LLMWrapper");

// ============================================================
// Agent Layer Service
// ============================================================

/**
 * AgentLoopRegistry - Agent Loop Registry
 * Manages the memory storage of AgentLoopEntities
 */
export const AgentLoopRegistry: ServiceIdentifier<unknown> = Symbol("AgentLoopRegistry");

/**
 * AgentLoopExecutor - Agent Loop Executor
 * Responsible for executing the iterative loop of the Agent tool, creating an independent message history with each execution.
 */
export const AgentLoopExecutor: ServiceIdentifier<unknown> = Symbol("AgentLoopExecutor");

/**
 * AgentLoopCoordinator - Agent Loop Lifecycle Coordinator
 * Coordinates the full lifecycle management of AgentLoopEntities
 */
export const AgentLoopCoordinator: ServiceIdentifier<unknown> = Symbol("AgentLoopCoordinator");

// ============================================================
// Skill Layer Service
// ============================================================

/**
 * SkillRegistry - Skill Registry
 * Manages the discovery, parsing, and querying of Skills
 */
export const SkillRegistry: ServiceIdentifier<unknown> = Symbol("SkillRegistry");

/**
 * SkillLoader - Skill Loader
 * Responsible for loading Skills, performing permission verification, and managing resource access control.
 */
export const SkillLoader: ServiceIdentifier<unknown> = Symbol("SkillLoader");
