/**
 * Node Processor Context Factory
 * Responsible for creating the appropriate processor context for different types of nodes
 *
 * Design Principles:
 * - Centralized management of node processor dependencies
 * - Creation of the right context based on the node type
 * - Simplification of the responsibilities of the NodeExecutionCoordinator
 * - Type-safe dependency injection with clear interfaces
 */

import type { RuntimeNode } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { UserInteractionHandler } from "@wf-agent/types";
import type { HumanRelayHandler } from "@wf-agent/types";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { WorkflowExecutionBuilder } from "../factories/workflow-execution-builder.js";
import type { WorkflowExecutor } from "../executors/workflow-executor.js";
import type { LLMWrapper } from "../../../core/llm/wrapper.js";
import type { ToolPermissionManager } from "../../../core/coordinators/tool-permission-manager.js";
import type { RejectionMessageBuilder } from "../../../core/coordinators/rejection-message-builder.js";
import { LLMExecutionCoordinator } from "../coordinators/llm-execution-coordinator.js";
import { ExecutionError } from "@wf-agent/types";

/**
 * Node Processor Context Factory Configuration
 */
export interface NodeHandlerContextFactoryConfig {
  /** Event Manager */
  eventManager: EventRegistry;
  /** LLM Execution Coordinator */
  llmCoordinator: LLMExecutionCoordinator;
  /** LLM Wrapper (required for LLM nodes to access profiles) */
  llmWrapper: LLMWrapper;
  /** Dialogue Manager */
  conversationManager: ConversationSession;
  /** User Interaction Handler (optional) */
  userInteractionHandler?: UserInteractionHandler;
  /** Manual Relay Processor (optional) */
  humanRelayHandler?: HumanRelayHandler;
  /** Tool Services (Optional) */
  toolService?: ToolRegistry;
  /** Agent Loop Executor Factory (optional) */
  agentLoopExecutorFactory?: unknown;
  /** Workflow Execution Registry (optional) */
  workflowExecutionRegistry?: WorkflowExecutionRegistry;
  /** Workflow Execution Builder (optional, required for FORK nodes) */
  executionBuilder?: WorkflowExecutionBuilder;
  /** Workflow Executor (optional, required for FORK nodes) */
  workflowExecutor?: WorkflowExecutor;
  /** Tool Permission Manager (optional, required for TOOL_VISIBILITY nodes) */
  permissionManager?: ToolPermissionManager;
  /** Rejection Message Builder (optional, required for TOOL_VISIBILITY nodes) */
  rejectionBuilder?: RejectionMessageBuilder;
}

/**
 * Node Processor Context Factory
 *
 * Responsibilities:
 * - Creates the appropriate processor context based on the node type.
 * - Manages processor dependencies in a centralized manner.
 * - Ensures that all necessary dependencies are in place.
 */
export class NodeHandlerContextFactory {
  constructor(private config: NodeHandlerContextFactoryConfig) {}

  /**
   * Create a node processor context
   *
   * @param node Node definition
   * @param executionEntity WorkflowExecution entity
   * @returns Processor context with type-safe properties based on node type
   * @throws ExecutionError When required dependencies are missing
   */
  createHandlerContext(node: RuntimeNode, executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    const contextCreator = this.contextCreators.get(node.type);
    
    if (!contextCreator) {
      // Other node types do not require any special context.
      return {};
    }

    return contextCreator(node, executionEntity);
  }

  /**
   * Context creator functions mapped by node type
   */
  private contextCreators: Map<string, (node: RuntimeNode, executionEntity: WorkflowExecutionEntity) => Record<string, unknown>> = new Map([
    ["USER_INTERACTION", (node, entity) => this.createUserInteractionContext(node, entity)],
    ["CONTEXT_PROCESSOR", (_node, entity) => this.createContextProcessorContext(entity)],
    ["LLM", (node, entity) => this.createLLMContext(node, entity)],
    ["AGENT_LOOP", (node, entity) => this.createAgentLoopContext(node, entity)],
    ["TOOL_VISIBILITY", (node, entity) => this.createToolVisibilityContext(node, entity)],
    ["FORK", (node, entity) => this.createForkContext(node, entity)],
    ["SUBGRAPH", (node, entity) => this.createSubgraphContext(node, entity)],
    ["START_FROM_TRIGGER", (_node, entity) => this.createStartFromTriggerContext(entity)],
  ]);

  /**
   * Create a user interaction node context
   * @param node Node definition (for error reporting)
   * @param executionEntity WorkflowExecution entity (for error reporting)
   * @returns Context with userInteractionHandler and conversationManager
   * @throws ExecutionError When userInteractionHandler is not provided
   */
  private createUserInteractionContext(
    node: RuntimeNode,
    executionEntity: WorkflowExecutionEntity,
  ): Record<string, unknown> {
    if (!this.config.userInteractionHandler) {
      throw new ExecutionError(
        "UserInteractionHandler is required for USER_INTERACTION node",
        node.id,
        executionEntity.getWorkflowId(),
      );
    }

    return {
      userInteractionHandler: this.config.userInteractionHandler,
      conversationManager: this.config.conversationManager,
    };
  }

  /**
   * Create a Context Processor node context
   * @param executionEntity WorkflowExecution entity for context operations
   * @returns Context with conversationManager, executionEntity, and workflowExecutionRegistry
   * @throws ExecutionError When conversationManager is not provided
   */
  private createContextProcessorContext(executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    if (!this.config.conversationManager) {
      throw new ExecutionError(
        "ConversationManager is required for CONTEXT_PROCESSOR node",
        "CONTEXT_PROCESSOR",
        executionEntity.getWorkflowId(),
      );
    }

    return {
      conversationManager: this.config.conversationManager,
      executionEntity: executionEntity,
      workflowExecutionRegistry: this.config.workflowExecutionRegistry,
    };
  }

  /**
   * Create LLM node context
   * @param node Node definition (for error reporting)
   * @param executionEntity WorkflowExecution entity (for error reporting)
   * @returns Context with llmCoordinator, llmWrapper, eventManager, conversationManager, and optional humanRelayHandler
   * @throws ExecutionError When required dependencies are not provided
   */
  private createLLMContext(node: RuntimeNode, executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    if (!this.config.llmWrapper) {
      throw new ExecutionError(
        "LLMWrapper is required for LLM node",
        node.id,
        executionEntity.getWorkflowId(),
      );
    }

    return {
      llmCoordinator: this.config.llmCoordinator,
      llmWrapper: this.config.llmWrapper,
      eventManager: this.config.eventManager,
      conversationManager: this.config.conversationManager,
      humanRelayHandler: this.config.humanRelayHandler,
    };
  }

  /**
   * Create a tool visibility node context
   * @param node Node definition (for error reporting)
   * @param executionEntity WorkflowExecution entity (for error reporting)
   * @returns Context with permissionManager and rejectionBuilder
   * @throws ExecutionError When permissionManager or rejectionBuilder is not provided
   */
  private createToolVisibilityContext(node: RuntimeNode, executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    if (!this.config.permissionManager) {
      throw new ExecutionError(
        "ToolPermissionManager is required for TOOL_VISIBILITY node",
        node.id,
        executionEntity.getWorkflowId(),
      );
    }

    if (!this.config.rejectionBuilder) {
      throw new ExecutionError(
        "RejectionMessageBuilder is required for TOOL_VISIBILITY node",
        node.id,
        executionEntity.getWorkflowId(),
      );
    }

    return {
      permissionManager: this.config.permissionManager,
      rejectionBuilder: this.config.rejectionBuilder,
    };
  }

  /**
   * Create an Agent Loop node context
   * @param node Node definition (for error reporting)
   * @param executionEntity WorkflowExecution entity (for error reporting)
   * @returns Context with agentLoopExecutor, llmCoordinator, conversationManager, and eventManager
   * @throws ExecutionError When agentLoopExecutorFactory is not provided
   */
  private createAgentLoopContext(node: RuntimeNode, executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    if (!this.config.agentLoopExecutorFactory) {
      throw new ExecutionError(
        "AgentLoopExecutorFactory is required for AGENT_LOOP node",
        node.id,
        executionEntity.getWorkflowId(),
      );
    }

    return {
      agentLoopExecutor: (
        this.config.agentLoopExecutorFactory as { create: () => unknown }
      ).create(),
      llmCoordinator: this.config.llmCoordinator,
      conversationManager: this.config.conversationManager,
      eventManager: this.config.eventManager,
      // The toolCallExecutor here is usually available in the NodeExecutionCoordinator via the Identifier
      // But in the handler, we need to make sure it's passed in.
      // Note: agent-loop-handler uses agentLoopExecutor to execute the loop.
    };
  }

  /**
   * Create a FORK node context
   * @param node Node definition (for error reporting)
   * @param executionEntity WorkflowExecution entity (for error reporting)
   * @returns Context with executionBuilder and workflowExecutor
   * @throws ExecutionError When executionBuilder or workflowExecutor is not provided
   */
  private createForkContext(node: RuntimeNode, executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    if (!this.config.executionBuilder) {
      throw new ExecutionError(
        "WorkflowExecutionBuilder is required for FORK node",
        node.id,
        executionEntity.getWorkflowId(),
      );
    }

    if (!this.config.workflowExecutor) {
      throw new ExecutionError(
        "WorkflowExecutor is required for FORK node",
        node.id,
        executionEntity.getWorkflowId(),
      );
    }

    return {
      executionBuilder: this.config.executionBuilder,
      workflowExecutor: this.config.workflowExecutor,
    };
  }

  /**
   * Create a SUBGRAPH node context
   * @param node Node definition (for error reporting)
   * @param executionEntity WorkflowExecution entity (for error reporting)
   * @returns Context with executionBuilder and workflowExecutor
   * @throws ExecutionError When executionBuilder or workflowExecutor is not provided
   */
  private createSubgraphContext(node: RuntimeNode, executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    if (!this.config.executionBuilder) {
      throw new ExecutionError(
        "WorkflowExecutionBuilder is required for SUBGRAPH node",
        node.id,
        executionEntity.getWorkflowId(),
      );
    }

    if (!this.config.workflowExecutor) {
      throw new ExecutionError(
        "WorkflowExecutor is required for SUBGRAPH node",
        node.id,
        executionEntity.getWorkflowId(),
      );
    }

    return {
      executionBuilder: this.config.executionBuilder,
      workflowExecutor: this.config.workflowExecutor,
    };
  }

  /**
   * Create a START_FROM_TRIGGER node context
   * @param _executionEntity WorkflowExecution entity (unused, kept for signature consistency)
   * @returns Context with optional triggerInput and conversationManager
   */
  private createStartFromTriggerContext(_executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    return {
      conversationManager: this.config.conversationManager,
      // Note: triggerInput should be passed from the external trigger mechanism,
      // not created here. It will be injected by the caller when executing the node.
    };
  }
}
