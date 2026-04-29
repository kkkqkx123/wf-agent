/**
 * Node Processor Context Factory
 * Responsible for creating the appropriate processor context for different types of nodes
 *
 * Design Principles:
 * - Centralized management of node processor dependencies
 * - Creation of the right context based on the node type
 * - Simplification of the responsibilities of the NodeExecutionCoordinator
 */

import type { Node } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { UserInteractionHandler } from "@wf-agent/types";
import type { HumanRelayHandler } from "@wf-agent/types";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
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
  /** Dialogue Manager */
  conversationManager: ConversationSession;
  /** User Interaction Handler (optional) */
  userInteractionHandler?: UserInteractionHandler;
  /** Manual Relay Processor (optional) */
  humanRelayHandler?: HumanRelayHandler;
  /** Tool Context Store (optional) */
  toolContextStore?: unknown;
  /** Tool Services (Optional) */
  toolService?: unknown;
  /** Agent Loop Executor Factory (optional) */
  agentLoopExecutorFactory?: unknown;
  /** Thread Registry (optional) */
  executionRegistry?: unknown;
  /** Workflow Execution Registry (optional) */
  workflowExecutionRegistry?: unknown;
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
   * @param executionEntity Thread entity
   * @returns Processor context
   * @throws ExecutionError When required dependencies are missing
   */
  createHandlerContext(node: Node, executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    switch (node.type) {
      case "USER_INTERACTION":
        return this.createUserInteractionContext(node, executionEntity);

      case "CONTEXT_PROCESSOR":
        return this.createContextProcessorContext(executionEntity);

      case "LLM":
        return this.createLLMContext();

      case "AGENT_LOOP":
        return this.createAgentLoopContext(node, executionEntity);

      case "ADD_TOOL":
        return this.createAddToolContext(node, executionEntity);

      default:
        // Other node types do not require any special context.
        return {};
    }
  }

  /**
   * Create a user interaction node context
   */
  private createUserInteractionContext(
    node: Node,
    executionEntity: WorkflowExecutionEntity,
  ): Record<string, unknown> {
    if (!this.config.userInteractionHandler) {
      throw new ExecutionError(
        "UserInteractionHandler is not provided",
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
   */
  private createContextProcessorContext(executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    return {
      conversationManager: this.config.conversationManager,
      executionEntity: executionEntity,
      workflowExecutionRegistry: this.config.executionRegistry,
    };
  }

  /**
   * Create LLM node context
   */
  private createLLMContext(): Record<string, unknown> {
    return {
      llmCoordinator: this.config.llmCoordinator,
      eventManager: this.config.eventManager,
      conversationManager: this.config.conversationManager,
      humanRelayHandler: this.config.humanRelayHandler,
    };
  }

  /**
   * Create a tool to add node context
   */
  private createAddToolContext(node: Node, executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    if (!this.config.toolContextStore || !this.config.toolService) {
      throw new ExecutionError(
        "ToolContextStore or ToolRegistry is not provided",
        node.id,
        executionEntity.getWorkflowId(),
      );
    }

    return {
      toolContextStore: this.config.toolContextStore,
      toolService: this.config.toolService,
      eventManager: this.config.eventManager,
      executionEntity,
    };
  }

  /**
   * Create an Agent Loop node context
   */
  private createAgentLoopContext(node: Node, executionEntity: WorkflowExecutionEntity): Record<string, unknown> {
    if (!this.config.agentLoopExecutorFactory) {
      throw new ExecutionError(
        "AgentLoopExecutorFactory is not provided",
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
}
