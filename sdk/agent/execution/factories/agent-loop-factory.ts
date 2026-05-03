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
 * ## Responsibilities
 *
 * 1. **Create New Instances**: Build `AgentLoopEntity` from `AgentLoopConfig`
 * 2. **Restore from Checkpoints**: Rebuild entity from serialized state + re-provided config
 * 3. **Parent-Child Management**: Register with parent workflow for lifecycle tracking
 *
 * ## Architecture Context
 *
 * This factory bridges the gap between configuration and runtime:
 * 
 * ```
 * AgentLoopConfig (with functions)
 *     ↓ Factory.create()
 * AgentLoopEntity {
 *   config: AgentLoopConfig       ← Injected here
 *   state: AgentLoopState         ← Created fresh or restored
 *   conversationManager           ← Created fresh
 *   variableStateManager          ← Created fresh
 * }
 * ```
 *
 * ## Checkpoint Restoration Flow
 *
 * When restoring from checkpoint:
 * 1. Application calls `AgentLoopFactory.fromCheckpoint(checkpointId, config)`
 * 2. Factory loads serialized `AgentLoopState` from storage
 * 3. Factory creates NEW `AgentLoopEntity` with:
 *    - Re-provided `config` (application must supply callbacks)
 *    - Restored `state` (from checkpoint snapshot)
 *    - Fresh managers (conversation, variables)
 * 4. Entity resumes execution from saved iteration
 *
 * ## Why Config Must Be Re-provided
 *
 * `AgentLoopConfig` contains functions (`transformContext`, `convertToLlm`) that cannot be
 * serialized to checkpoints. Therefore:
 * - ❌ Config is NOT saved to checkpoint
 * - ✅ Application must provide config when calling `fromCheckpoint()`
 * - ✅ Only `AgentLoopState` is restored from checkpoint
 *
 * This design ensures:
 * - Functions can be updated between runs (e.g., bug fixes)
 * - Different configs can restore same state (flexibility)
 * - No serialization issues with closures/callbacks
 *
 * @see AgentLoopEntity - The entity created by this factory
 * @see AgentLoopConfig - Configuration required for creation
 * @see AgentLoopState - State restored from checkpoints
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
