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
import type {
  LLMMessage,
  AgentLoopRuntimeConfig,
  ID,
  IterationRecord,
  TokenUsageStats,
  MessageMarkMap,
  AgentLoopStateSnapshot,
} from "@wf-agent/types";
import { getAvailableTools, AgentLoopStatus } from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../../../workflow/stores/workflow-execution-registry.js";
import { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import type { GlobalContext } from "../../../core/global-context.js";
import { AgentLoopCheckpointCoordinator } from "../../checkpoint/index.js";
import type { ExecutionHierarchyRegistry } from "../../../core/registry/execution-hierarchy-registry.js";
import type { CheckpointDependencies } from "../../checkpoint/checkpoint-coordinator.js";
import { ConversationSession } from "../../../core/messaging/conversation-session.js";
import { AgentStateCoordinator } from "../../state-managers/agent-state-coordinator.js";

const logger = createContextualLogger({ component: "AgentLoopFactory" });

/**
 * AgentLoopEntity creation options
 */
export interface AgentLoopEntityOptions {
  /** Initial message */
  initialMessages?: LLMMessage[];
  /** Dialogue Manager */
  conversationManager?: ConversationSession;
  /** Parent Execution ID */
  parentExecutionId?: ID;
  /** Node ID */
  nodeId?: ID;
}

/**
 * Memory for fromConversationHistory restoration
 * Allows partial state recovery without full checkpoint
 */
export interface ConversationHistoryMemory {
  /** Iteration history records (optional, derived from messages if not provided) */
  iterationHistory?: IterationRecord[];
  /** Token usage statistics */
  tokenUsage?: TokenUsageStats | null;
  /** Current request token usage */
  currentRequestUsage?: TokenUsageStats | null;
  /** Message visibility mark map */
  markMap?: MessageMarkMap;
  /** Turn-based execution states */
  turnStates?: Record<number, Record<string, unknown>>;
  /** Parent execution context */
  parentContext?: {
    parentType: "WORKFLOW" | "AGENT_LOOP";
    parentId: ID;
    nodeId?: ID;
  };
}

/**
 * AgentLoopFactory - Agent Loop Factory Class
 *
 * ## Responsibilities
 *
 * 1. **Create New Instances**: Build `AgentLoopEntity` from `AgentLoopRuntimeConfig`
 * 2. **Restore from Checkpoints**: Rebuild entity from serialized state + re-provided config
 * 3. **Restore from Messages**: Create entity from message array + config (lightweight restore)
 * 4. **Restore from Conversation History**: Create entity with full conversation state recovery
 * 5. **Parent-Child Management**: Register with parent workflow for lifecycle tracking
 *
 * ## Architecture Context
 *
 * This factory bridges the gap between configuration and runtime:
 *
 * ```
 * AgentLoopRuntimeConfig (with functions)
 *     ↓ Factory.create()
 * AgentLoopEntity {
 *   config: AgentLoopRuntimeConfig  ← Injected here
 *   state: AgentLoopState         ← Created fresh or restored
 *   conversationManager           ← Created fresh or restored
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
 * `AgentLoopRuntimeConfig` contains functions (`transformContext`, `convertToLlm`) that cannot be
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
 * @see AgentLoopRuntimeConfig - Configuration required for creation
 * @see AgentLoopState - State restored from checkpoints
 */
export class AgentLoopFactory {
  /**
   * Create a new AgentLoopEntity instance
   * @param globalContext: Global context for accessing DI container
   * @param config: Loop configuration
   * @param options: Creation options
   * @returns: AgentLoopEntity instance and AgentStateCoordinator
   */
  static async create(
    globalContext: GlobalContext,
    config: AgentLoopRuntimeConfig,
    options: AgentLoopEntityOptions = {},
  ): Promise<{ entity: AgentLoopEntity; stateCoordinator: AgentStateCoordinator }> {
    const id = `agent-loop-${randomUUID()}`;

    // Get execution hierarchy registry from DI container
    const registry = globalContext.container.get(
      Identifiers.ExecutionHierarchyRegistry,
    ) as ExecutionHierarchyRegistry;

    const entity = new AgentLoopEntity(id, config, undefined, undefined, registry);

    logger.info("Creating new Agent Loop entity", {
      agentLoopId: id,
      maxIterations: config.maxIterations,
      toolsCount: getAvailableTools(config.availableTools).length,
      profileId: config.profileId || "DEFAULT",
    });

    // Create ConversationSession externally
    const conversationSession = new ConversationSession({
      executionId: id,
      initialMessages: options.initialMessages ?? [],
    });

    // Create AgentStateCoordinator wrapping the ConversationSession
    const stateCoordinator = new AgentStateCoordinator({
      conversationManager: conversationSession,
    });

    // Build initial messages if config provides them
    const initialMessages: LLMMessage[] = [];

    // Add system prompt if provided
    if (config.systemPrompt) {
      initialMessages.push({
        role: "system",
        content: config.systemPrompt,
      });
    }

    // Add initial user message if provided
    if (config.initialUserMessage) {
      initialMessages.push({
        role: "user",
        content: config.initialUserMessage,
      });
    }

    // Add any additional initial messages from options
    if (options.initialMessages && options.initialMessages.length > 0) {
      initialMessages.push(...options.initialMessages);
    }

    // Set messages on state coordinator
    if (initialMessages.length > 0) {
      stateCoordinator.setMessages(initialMessages);
      logger.debug("Agent Loop initialized with initial messages", {
        agentLoopId: id,
        messageCount: initialMessages.length,
      });
    }

    // Set parent context using new unified API (Phase 4)
    if (options.parentExecutionId) {
      entity.setParentContext({
        parentType: "WORKFLOW",
        parentId: options.parentExecutionId,
        nodeId: options.nodeId,
      });

      logger.debug("Agent Loop set with parent context", {
        agentLoopId: id,
        parentExecutionId: options.parentExecutionId,
        nodeId: options.nodeId,
      });
    }

    // Register with parent Execution for lifecycle management
    if (options.parentExecutionId) {
      await this.registerWithParentExecution(globalContext, id, options.parentExecutionId);
    }

    logger.info("Agent Loop entity created successfully", { agentLoopId: id });
    return { entity, stateCoordinator };
  }

  /**
   * Register AgentLoop with parent Execution for lifecycle management
   * @param globalContext Global context for accessing DI container
   * @param agentLoopId AgentLoop ID
   * @param parentExecutionId Parent Execution ID
   */
  private static async registerWithParentExecution(
    globalContext: GlobalContext,
    agentLoopId: string,
    parentExecutionId: string,
  ): Promise<void> {
    try {
      const executionRegistry = globalContext.container.get(
        Identifiers.WorkflowExecutionRegistry,
      ) as WorkflowExecutionRegistry;

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
   *
   * @param checkpointId Checkpoint ID
   * @param config AgentLoopRuntimeConfig (must be re-provided by application, NOT from checkpoint)
   * @param dependencies Checkpoint dependencies
   * @returns AgentLoopEntity instance
   */
  static async fromCheckpoint(
    checkpointId: string,
    config: AgentLoopRuntimeConfig,
    dependencies: {
      saveCheckpoint: (checkpoint: unknown) => Promise<string>;
      getCheckpoint: (id: string) => Promise<unknown>;
      listCheckpoints: (agentLoopId: string) => Promise<string[]>;
      deltaConfig?: unknown;
    },
  ): Promise<AgentLoopEntity> {
    logger.info("Restoring Agent Loop from checkpoint", { checkpointId });

    const coordinator = new AgentLoopCheckpointCoordinator(config);
    const entity = await coordinator.restoreFromCheckpoint(
      checkpointId,
      dependencies as CheckpointDependencies,
    );

    logger.info("Agent Loop restored from checkpoint successfully", {
      agentLoopId: entity.id,
      checkpointId,
      iteration: entity.state.currentIteration,
    });

    return entity;
  }

  /**
   * Create AgentLoopEntity from a message array (no checkpoint required)
   *
   * This is a lightweight alternative to checkpoint-based restoration:
   * - Accepts an existing LLMMessage[] array + AgentLoopRuntimeConfig
   * - Creates a fresh AgentLoopEntity with the messages pre-loaded
   * - Derives iteration count and tool call count from message pairs
   * - Useful for: importing conversations, testing, debugging, external integration
   *
   * Limitations:
   * - Cannot restore tool execution records, token usage, or turn states
   * - Messages are set directly; no initial message build is performed
   *
   * @param id Entity ID (auto-generated if not provided)
   * @param messages Message array to restore
   * @param config Runtime configuration
   * @returns AgentLoopEntity instance ready for execution
   */
  static fromMessages(
    messages: LLMMessage[],
    config: AgentLoopRuntimeConfig,
    id?: string,
  ): { entity: AgentLoopEntity; stateCoordinator: AgentStateCoordinator } {
    const entityId = id || `agent-loop-msg-${randomUUID()}`;
    const entity = new AgentLoopEntity(entityId, config);

    // Create ConversationSession externally
    const conversationSession = new ConversationSession({
      executionId: entityId,
      initialMessages: messages,
    });

    // Create AgentStateCoordinator wrapping the ConversationSession
    const stateCoordinator = new AgentStateCoordinator({
      conversationManager: conversationSession,
    });

    // Derive iteration count and tool call count from message pairs
    let iterationCount = 0;
    let toolCallCount = 0;
    for (const msg of messages) {
      if (msg.role === "assistant" && msg.toolCalls && msg.toolCalls.length > 0) {
        for (const tc of msg.toolCalls) {
          // Each tool call in an assistant message increments the count
          const hasToolResult = messages.some(m => m.role === "tool" && m.toolCallId === tc.id);
          if (hasToolResult) {
            toolCallCount++;
          }
        }
      }
      // Count user messages (each user message after system starts a new "turn")
      // A simpler heuristic: count (user + tool/assistant pairs)
      if (msg.role === "assistant") {
        iterationCount++;
      }
    }

    // Set derived state
    entity.state.status = AgentLoopStatus.CREATED;
    // Store the derived counts directly in the private fields
    // by using the snapshot/restore mechanism
    const snapshot = entity.state.createSnapshot();
    snapshot.currentIteration = Math.max(0, iterationCount - 1); // -1 because 0-based
    snapshot.toolCallCount = toolCallCount;
    entity.state.restoreFromSnapshot(snapshot);

    logger.info("Agent Loop entity created from messages", {
      agentLoopId: entityId,
      messageCount: messages.length,
      derivedIterations: entity.state.currentIteration,
      derivedToolCalls: entity.state.toolCallCount,
    });

    return { entity, stateCoordinator };
  }

  /**
   * Create AgentLoopEntity from conversation history with full state recovery
   *
   * This provides a richer restoration path than `fromMessages()`:
   * - Accepts messages + config + optional memory (iterationHistory, tokenUsage, markMap, turnStates)
   * - Creates a fresh AgentLoopEntity with fully restored ConversationSession
   * - Restores iteration history and tool call records from memory if provided
   * - Derives iteration/tool call counts from messages as fallback
   * - Restores token usage state, mark map, and turn states if provided
   *
   * Use cases:
   * 1. **Conversation import**: Importing from an external conversation store
   * 2. **Manual restore**: Restoring agent state from externally-stored conversation data
   * 3. **Branching**: Creating a new execution from a previous conversation state
   * 4. **Testing**: Creating controlled test scenarios with specific message histories
   *
   * @param messages Message array (the conversation history)
   * @param config Runtime configuration
   * @param memory Optional state memory for richer restoration
   * @param id Entity ID (auto-generated if not provided)
   * @returns AgentLoopEntity instance ready for execution
   */
  static fromConversationHistory(
    messages: LLMMessage[],
    config: AgentLoopRuntimeConfig,
    memory?: ConversationHistoryMemory,
    id?: string,
  ): { entity: AgentLoopEntity; stateCoordinator: AgentStateCoordinator } {
    const entityId = id || `agent-loop-conv-${randomUUID()}`;

    // Create fresh entity
    const entity = new AgentLoopEntity(entityId, config);

    // Restore iteration history and tool call counts
    let currentIteration: number;
    let toolCallCount: number;

    if (memory?.iterationHistory && memory.iterationHistory.length > 0) {
      // If iteration history is provided, use its last iteration as current
      const lastRecord = memory.iterationHistory[memory.iterationHistory.length - 1];
      currentIteration = lastRecord ? lastRecord.iteration : 0;

      // Count all tool calls across all iterations
      toolCallCount = memory.iterationHistory.reduce(
        (count, record) => count + record.toolCalls.length,
        0,
      );
    } else {
      // Fallback: derive from messages
      let iterCount = 0;
      let toolCnt = 0;
      for (const msg of messages) {
        if (msg.role === "assistant" && msg.toolCalls && msg.toolCalls.length > 0) {
          for (const tc of msg.toolCalls) {
            const hasToolResult = messages.some(m => m.role === "tool" && m.toolCallId === tc.id);
            if (hasToolResult) {
              toolCnt++;
            }
          }
        }
        if (msg.role === "assistant") {
          iterCount++;
        }
      }
      currentIteration = Math.max(0, iterCount - 1);
      toolCallCount = toolCnt;
    }

    // Create and restore state snapshot
    const snapshot: AgentLoopStateSnapshot = {
      status: AgentLoopStatus.CREATED,
      currentIteration,
      toolCallCount,
      startTime: null,
      endTime: null,
      error: null,
      iterationHistory: memory?.iterationHistory
        ? memory.iterationHistory.map(record => ({
            ...record,
            toolCalls: record.toolCalls.map(tc => ({ ...tc })),
          }))
        : undefined,
    };
    entity.state.restoreFromSnapshot(snapshot);

    // Restore ConversationSession with full state
    const restoredSession = new ConversationSession({
      executionId: entityId,
      initialMessages: messages,
    });

    // Restore mark map if provided
    if (memory?.markMap) {
      restoredSession.setMarkMap(memory.markMap);
    }

    // Restore token usage state if provided
    if (memory?.tokenUsage !== undefined) {
      restoredSession.setTokenUsageState(memory.tokenUsage, memory.currentRequestUsage ?? null);
    }

    // Restore turn states if provided
    if (memory?.turnStates) {
      Object.entries(memory.turnStates).forEach(([turnIndex, state]) => {
        Object.entries(state as Record<string, unknown>).forEach(([key, value]) => {
          restoredSession.setTurnState(Number(turnIndex), key, value);
        });
      });
    }

    // Create AgentStateCoordinator wrapping the restored session
    const stateCoordinator = new AgentStateCoordinator({
      conversationManager: restoredSession,
    });

    // Set parent context if provided in memory
    if (memory?.parentContext) {
      entity.setParentContext(memory.parentContext);
    }

    logger.info("Agent Loop entity created from conversation history", {
      agentLoopId: entityId,
      messageCount: messages.length,
      derivedIterations: entity.state.currentIteration,
      derivedToolCalls: entity.state.toolCallCount,
      hasIterationHistory: !!memory?.iterationHistory,
      hasTokenUsage: !!memory?.tokenUsage,
      hasMarkMap: !!memory?.markMap,
      hasTurnStates: !!memory?.turnStates,
    });

    return { entity, stateCoordinator };
  }

  /**
   * Create AgentLoopEntity from checkpoint with partial state overrides
   *
   * This provides **reference-based partial state override** (P4):
   * - Loads state from a checkpoint as the baseline
   * - Allows overriding specific fields (messages, iteration history, status, etc.)
   * - Useful for branching, retrying with modifications, debugging
   *
   * The override works at the AgentLoopStateSnapshot level:
   * - Messages overrides are applied to the ConversationSession after creation
   * - State overrides (iteration, toolCalls, status) are applied to AgentLoopState
   *
   * @param checkpointId Source checkpoint ID
   * @param config Runtime configuration (must be re-provided)
   * @param dependencies Checkpoint dependencies
   * @param overrides Partial state overrides (optional)
   * @returns AgentLoopEntity with overridden state
   */
  static async fromCheckpointWithOverrides(
    checkpointId: string,
    config: AgentLoopRuntimeConfig,
    dependencies: {
      saveCheckpoint: (checkpoint: unknown) => Promise<string>;
      getCheckpoint: (id: string) => Promise<unknown>;
      listCheckpoints: (agentLoopId: string) => Promise<string[]>;
      deltaConfig?: unknown;
    },
    overrides?: {
      /** Override messages in the restored entity */
      messages?: LLMMessage[];
      /** Override specific state snapshot fields */
      stateOverrides?: Partial<
        Pick<
          AgentLoopStateSnapshot,
          "currentIteration" | "toolCallCount" | "status" | "error" | "startTime" | "endTime"
        >
      >;
      /** Override mark map */
      markMap?: MessageMarkMap;
      /** Override token usage */
      tokenUsage?: TokenUsageStats | null;
      /** Override current request usage */
      currentRequestUsage?: TokenUsageStats | null;
      /** Override turn states */
      turnStates?: Record<number, Record<string, unknown>>;
    },
  ): Promise<{ entity: AgentLoopEntity; stateCoordinator: AgentStateCoordinator }> {
    logger.info("Restoring Agent Loop from checkpoint with overrides", { checkpointId });

    // First, restore the entity from the checkpoint as baseline
    const coordinator = new AgentLoopCheckpointCoordinator(config);
    const entity = await coordinator.restoreFromCheckpoint(
      checkpointId,
      dependencies as CheckpointDependencies,
    );

    // Create ConversationSession and AgentStateCoordinator for the restored entity
    const conversationSession = new ConversationSession({
      executionId: entity.id,
      initialMessages: overrides?.messages ?? [],
    });
    const stateCoordinator = new AgentStateCoordinator({
      conversationManager: conversationSession,
    });

    // Apply state overrides if provided
    if (overrides) {
      // Apply state snapshot overrides
      if (overrides.stateOverrides) {
        const currentSnapshot = entity.state.createSnapshot();
        const mergedSnapshot: AgentLoopStateSnapshot = {
          ...currentSnapshot,
          ...overrides.stateOverrides,
        };
        entity.state.restoreFromSnapshot(mergedSnapshot);

        logger.debug("Applied state overrides from checkpoint", {
          agentLoopId: entity.id,
          overriddenFields: Object.keys(overrides.stateOverrides),
        });
      }

      // Apply mark map override to state coordinator
      if (overrides.markMap) {
        stateCoordinator.setMarkMap(overrides.markMap);
        logger.debug("Applied mark map override", {
          agentLoopId: entity.id,
        });
      }

      // Apply token usage override to state coordinator
      if (overrides.tokenUsage !== undefined) {
        stateCoordinator.setTokenUsageState(overrides.tokenUsage, overrides.currentRequestUsage ?? null);
        logger.debug("Applied token usage override", {
          agentLoopId: entity.id,
        });
      }

      // Apply turn states override to state coordinator
      if (overrides.turnStates) {
        const session = stateCoordinator.getConversationManager();
        Object.entries(overrides.turnStates).forEach(([turnIndex, state]) => {
          Object.entries(state as Record<string, unknown>).forEach(([key, value]) => {
            session.setTurnState(Number(turnIndex), key, value);
          });
        });
        logger.debug("Applied turn states override", {
          agentLoopId: entity.id,
          turnCount: Object.keys(overrides.turnStates).length,
        });
      }
    }

    logger.info("Agent Loop restored from checkpoint with overrides", {
      agentLoopId: entity.id,
      checkpointId,
      iteration: entity.state.currentIteration,
      hadOverrides: !!overrides,
    });

    return { entity, stateCoordinator };
  }
}
