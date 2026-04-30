/**
 * AgentLoopFactory - The Agent Loop factory class
 *
 * Responsible for creating instances of AgentLoopEntity, including:
 * - Creating new instances
 * - Restoring from checkpoints
 * - Registering with parent workflow execution for lifecycle management
 *
 * Design principles:
 * - Centralized management of instance creation logic
 * - Decoupled from the entity classes
 * - Supports multiple creation methods
 * - Automatic parent-child relationship management
 */

import { randomUUID } from "crypto";
import type { LLMMessage, AgentLoopConfig, ID } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../workflow/stores/workflow-execution-registry.js";
import { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../../core/messaging/conversation-session.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import { AgentLoopCheckpointCoordinator } from "../../checkpoint/index.js";

const logger = createContextualLogger({ component: "AgentLoopFactory" });

/**
 * AgentLoopEntity creation options
 */
export interface AgentLoopEntityOptions {
  /** Initial message */
  initialMessages?: LLMMessage[];
  /** Initial variables */
  initialVariables?: Record<string, unknown>;
  /** Dialogue Manager */
  conversationManager?: ConversationSession;
  /** Parent Execution ID */
  parentExecutionId?: ID;
  /** Node ID */
  nodeId?: ID;
}

/**
 * AgentLoopFactory - Agent Loop Factory Class
 *
 * Responsibilities:
 * - Creates new AgentLoopEntity instances.
 * - Recovers AgentLoopEntity instances from checkpoints.
 *
 * Design Principles:
 * - Factory Pattern: Centralized management of instance creation.
 * - Decoupling: Separates the creation logic from the entity class.
 * - Extensibility: Supports multiple creation methods.
 */
export class AgentLoopFactory {
  /**
   * Create a new AgentLoopEntity instance
   * @param config: Loop configuration
   * @param options: Creation options
   * @returns: AgentLoopEntity instance
   */
  static async create(
    config: AgentLoopConfig,
    options: AgentLoopEntityOptions = {},
  ): Promise<AgentLoopEntity> {
    const id = `agent-loop-${randomUUID()}`;
    const entity = new AgentLoopEntity(id, config);

    logger.info("Creating new Agent Loop entity", {
      agentLoopId: id,
      maxIterations: config.maxIterations,
      toolsCount: config.tools?.length || 0,
      profileId: config.profileId || "DEFAULT",
    });

    // Asynchronous initialization message
    await entity.initializeMessages({
      systemPrompt: config.systemPrompt,
      systemPromptTemplateId: config.systemPromptTemplateId,
      systemPromptTemplateVariables: config.systemPromptTemplateVariables,
      initialUserMessage: config.initialUserMessage,
      initialMessages: options.initialMessages,
    });

    // Initialize message history (via ConversationSession)
    // Note: The AgentLoopEntity constructor has already initialized the messages using buildInitialMessages
    // Only the options.initialMessages override needs to be handled here
    if (options.initialMessages && options.initialMessages.length > 0) {
      entity.setMessages(options.initialMessages);
      logger.debug("Agent Loop initialized with initial messages", {
        agentLoopId: id,
        messageCount: options.initialMessages.length,
      });
    }

    // Initialize variables
    if (options.initialVariables) {
      const variableCount = Object.keys(options.initialVariables).length;
      for (const [key, value] of Object.entries(options.initialVariables)) {
        entity.variableStateManager.setVariableValue(key, value, "workflowExecution");
      }
      logger.debug("Agent Loop initialized with variables", {
        agentLoopId: id,
        variableCount,
      });
    }

    // Set the parent Execution ID and node ID
    entity.parentExecutionId = options.parentExecutionId;
    entity.nodeId = options.nodeId;

    if (options.parentExecutionId || options.nodeId) {
      logger.debug("Agent Loop set with parent context", {
        agentLoopId: id,
        parentExecutionId: options.parentExecutionId,
        nodeId: options.nodeId,
      });
    }

    // Register with parent Execution for lifecycle management
    if (options.parentExecutionId) {
      await this.registerWithParentExecution(id, options.parentExecutionId);
    }

    logger.info("Agent Loop entity created successfully", { agentLoopId: id });
    return entity;
  }

  /**
   * Register AgentLoop with parent Execution for lifecycle management
   * @param agentLoopId AgentLoop ID
   * @param parentExecutionId Parent Execution ID
   */
  private static async registerWithParentExecution(
    agentLoopId: string,
    parentExecutionId: string,
  ): Promise<void> {
    try {
      const container = getContainer();
      const executionRegistry = container.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;

      if (executionRegistry) {
        const executionEntity = executionRegistry.get(parentExecutionId);
        if (executionEntity) {
          executionEntity.registerChildAgentLoop(agentLoopId);
          logger.debug("AgentLoop registered with parent Execution", {
            agentLoopId,
            parentExecutionId,
          });
        } else {
          logger.warn("Parent Workflow execution not found for AgentLoop registration", {
            agentLoopId,
            parentExecutionId,
          });
        }
      }
    } catch (error) {
      // Log error but don't throw - registration failure should not prevent creation
      logger.warn("Failed to register AgentLoop with parent Execution", {
        agentLoopId,
        parentExecutionId,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Restore the AgentLoopEntity instance from a checkpoint
   * @param checkpointId Checkpoint ID
   * @param dependencies Checkpoint dependencies
   * @returns AgentLoopEntity instance
   */
  static async fromCheckpoint(
    checkpointId: string,
    dependencies: {
      saveCheckpoint: (checkpoint: unknown) => Promise<string>;
      getCheckpoint: (id: string) => Promise<unknown>;
      listCheckpoints: (agentLoopId: string) => Promise<string[]>;
      deltaConfig?: unknown;
    },
  ): Promise<AgentLoopEntity> {
    logger.info("Restoring Agent Loop from checkpoint", { checkpointId });

    const coordinator = new AgentLoopCheckpointCoordinator();
    const entity = await coordinator.restoreFromCheckpoint(
      checkpointId,
      dependencies as import("../../checkpoint/checkpoint-coordinator.js").CheckpointDependencies,
    );

    logger.info("Agent Loop restored from checkpoint successfully", {
      agentLoopId: entity.id,
      checkpointId,
      iteration: entity.state.currentIteration,
    });

    return entity;
  }
}
