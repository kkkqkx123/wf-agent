/**
 * Conversation Session
 * Extends MessageHistory to add Graph-specific features
 *
 * Graph-specific features:
 * 1. Token statistics and event triggering
 * 2. Tool description message management
 * 3. Integration with the Graph execution layer
 *
 * Features inherited from MessageHistory:
 * - Message history management
 * - Batch visibility control
 * - Snapshot/recovery
 */

import type { LLMMessage, TokenUsageStats, Tool, LLMUsage } from "@wf-agent/types";
import type { CheckpointStorageAdapter } from "@wf-agent/storage";
import { MessageHistory, type MessageHistoryState } from "./message-history.js";
import { TokenUsageTracker } from "../utils/token/token-usage-tracker.js";
import type { EventRegistry } from "../registry/event-registry.js";
import type { StateManager } from "../types/state-manager.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { emit } from "../utils/event/emit-event.js";
import { now } from "@wf-agent/common-utils";
import {
  buildTokenLimitExceededEvent,
  buildContextCompressionRequestedEvent,
  buildContextCompressionCompletedEvent,
} from "../utils/event/builders/index.js";
import { generateToolListDescription } from "../utils/tools/tool-description-generator.js";
import { executeOperation } from "../utils/messages/message-operation-utils.js";
import type { MessageOperationConfig, MessageOperationResult } from "@wf-agent/types";

const logger = createContextualLogger();

/**
 * Conversation Session Configuration
 */
export interface ConversationSessionConfig {
  /** Event Manager */
  eventManager?: EventRegistry;
  /** Execution ID */
  executionId?: string;
  /** Workflow Execution ID */
  workflowExecutionId?: string;
  /** Workflow ID */
  workflowId?: string;
  /** Token Limitation */
  tokenLimit?: number;
  /** Initial message */
  initialMessages?: LLMMessage[];
  /** Checkpoint Storage Callback for memory optimization */
  checkpointStorage?: CheckpointStorageAdapter;
}

/**
 * Dialogue State (for checkpoints)
 */
export interface ConversationState extends MessageHistoryState {
  /** Token Usage */
  tokenUsage?: TokenUsageStats | null;
  /** Usage of the Token for the current request */
  currentRequestUsage?: TokenUsageStats | null;
  /** Turn-based execution states (Persistent) */
  turnStates?: Record<number, Record<string, unknown>>;
}

/**
 * Conversation Session
 */
export class ConversationSession extends MessageHistory implements StateManager<ConversationState> {
  protected tokenUsageTracker: TokenUsageTracker;
  protected eventManager?: EventRegistry;
  protected executionId?: string;
  protected workflowId?: string;
  protected checkpointStorage?: CheckpointStorageAdapter;

  /** Turn-based execution state storage. Part of the persistent conversation state. */
  private turnStates: Map<number, Record<string, unknown>> = new Map();

  /**
   * Constructor
   * @param config Configuration options
   */
  constructor(config: ConversationSessionConfig = {}) {
    super({ initialMessages: config.initialMessages });
    this.eventManager = config.eventManager;
    this.executionId = config.executionId;
    this.workflowId = config.workflowId;
    this.checkpointStorage = config.checkpointStorage;

    this.tokenUsageTracker = new TokenUsageTracker({
      tokenLimit: config.tokenLimit,
    });
  }

  /**
   * Set checkpoint storage for memory optimization
   * @param storage Checkpoint storage callback
   */
  setCheckpointStorage(storage: CheckpointStorageAdapter): void {
    this.checkpointStorage = storage;
  }

  /**
   * Get checkpoint storage
   * @returns Checkpoint storage callback or undefined
   */
  getCheckpointStorage(): CheckpointStorageAdapter | undefined {
    return this.checkpointStorage;
  }

  /**
   * Start a new batch with automatic checkpoint support
   * If checkpointStorage is configured, will save previous batch to checkpoint and release from memory
   * @param boundaryIndex Boundary index (default is current message count)
   * @param keepInMemory Number of recent batches to keep in memory (default: 2)
   * @returns New batch number
   */
  async startNewBatchWithAutoCheckpoint(
    boundaryIndex?: number,
    keepInMemory: number = 2,
  ): Promise<number> {
    if (!this.checkpointStorage) {
      // No checkpoint storage configured, use standard mode
      return this.startNewBatch(boundaryIndex);
    }

    return this.startNewBatchWithCheckpoint(this.checkpointStorage, boundaryIndex, keepInMemory);
  }

  /**
   * Initialize resources
   */
  async initialize(): Promise<void> {
    // Historical records can be loaded here, among other things.
  }

  /**
   * Set context information
   * @param workflowId: Workflow ID
   * @param executionId: Execution ID
   */
  setContext(workflowId: string, executionId: string): void {
    this.workflowId = workflowId;
    this.executionId = executionId;
  }

  /**
   * Get execution ID
   * @returns Execution ID
   */
  getExecutionId(): string | undefined {
    return this.executionId;
  }

  /**
   * Get Token usage statistics
   * @returns Statistics on Token usage
   */
  getTokenUsage(): TokenUsageStats {
    const messages = this.getAllMessages();
    const tokenCount = this.tokenUsageTracker.getTokenUsage(messages);
    return (
      this.tokenUsageTracker.getCumulativeUsage() || {
        promptTokens: 0,
        completionTokens: 0,
        totalTokens: tokenCount,
      }
    );
  }

  /**
   * Check the usage of tokens and trigger an event.
   */
  async checkTokenUsage(): Promise<void> {
    const tokensUsed = this.getTokenUsage().totalTokens;

    // If the limit is exceeded, the Token limit event and the compression request event will be triggered.
    if (this.tokenUsageTracker.isTokenLimitExceeded(this.getAllMessages())) {
      await this.triggerTokenLimitEvent(tokensUsed);
      await this.triggerCompressionRequestedEvent(tokensUsed);
    }
  }

  /**
   * Trigger the Token Limit Event
   * @param tokensUsed: The number of tokens currently in use
   */
  private async triggerTokenLimitEvent(tokensUsed: number): Promise<void> {
    // 1. Send an event through the EventRegistry.
    if (this.eventManager && this.workflowId && this.executionId) {
      const event = buildTokenLimitExceededEvent({
        executionId: this.executionId,
        tokensUsed,
        tokenLimit: this.tokenUsageTracker["tokenLimit"],
        workflowId: this.workflowId,
      });
      await emit(this.eventManager, event);
    }

    // 2. Log warning messages.
    logger.warn(`Token limit exceeded: ${tokensUsed} > ${this.tokenUsageTracker["tokenLimit"]}`, {
      tokensUsed,
      tokenLimit: this.tokenUsageTracker["tokenLimit"],
      workflowId: this.workflowId,
      executionId: this.executionId,
      operation: "token_usage_check",
    });
  }

  /**
   * Trigger the context compression request event
   * @param tokensUsed The number of tokens currently in use
   */
  private async triggerCompressionRequestedEvent(tokensUsed: number): Promise<void> {
    if (this.eventManager && this.workflowId && this.executionId) {
      const event = buildContextCompressionRequestedEvent({
        executionId: this.executionId,
        tokensUsed,
        tokenLimit: this.tokenUsageTracker["tokenLimit"],
        workflowId: this.workflowId,
        stats: {
          messageCount: this.getAllMessages().length,
          lastMessageAt: now(),
        },
      });
      await emit(this.eventManager, event);
    }
  }

  /**
   * Execute the message operation and trigger the completion event.
   * @param operation: Message operation configuration
   * @returns: Operation result
   */
  async executeMessageOperation(
    operation: MessageOperationConfig,
    onAfterOperation?: (result: MessageOperationResult) => Promise<void>,
  ): Promise<MessageOperationResult> {
    const allMessages = this.getAllMessages();
    const markMap = this.getMarkMap();

    const result = await executeOperation(
      { messages: allMessages, markMap },
      operation,
      onAfterOperation,
    );

    // Update the internal state.
    this.clear();
    for (const msg of result.messages) {
      this.addMessage(msg);
    }
    this.setMarkMap(result.markMap);

    // Trigger the completion event
    if (this.eventManager && this.workflowId && this.executionId) {
      const event = buildContextCompressionCompletedEvent({
        executionId: this.executionId,
        workflowId: this.workflowId,
        tokensAfter: this.getTokenUsage().totalTokens,
      });
      await emit(this.eventManager, event);
    }

    return result;
  }

  /**
   * Get the description of the tool list (based on the currently available tools)
   * @param tools Tool list
   * @returns Tool description messages
   */
  getToolDescriptionMessage(tools: Tool[]): LLMMessage {
    return {
      role: "system",
      content: generateToolListDescription(tools, "list"),
    };
  }

  /**
   * Get the current usage of the Token for the request
   * @returns The current usage of the Token for the request
   */
  getCurrentRequestUsage(): TokenUsageStats {
    return (
      this.tokenUsageTracker.getCurrentRequestUsage() || {
        promptTokens: 0,
        completionTokens: 0,
        totalTokens: 0,
      }
    );
  }

  /**
   * Set the Token usage statistics status
   * Used for restoring the state from a checkpoint
   * @param cumulativeUsage: Cumulative Token usage statistics
   * @param currentRequestUsage: Current request Token usage statistics (optional)
   */
  setTokenUsageState(
    cumulativeUsage: TokenUsageStats | null,
    currentRequestUsage?: TokenUsageStats | null,
  ): void {
    this.tokenUsageTracker.setState(cumulativeUsage, currentRequestUsage);
  }

  /**
   * Update the statistics using the Token returned by the API
   * @param usage: Token usage data
   */
  updateTokenUsage(usage: LLMUsage): void {
    this.tokenUsageTracker.updateApiUsage(usage);
  }

  /**
   * Complete the token statistics for the current request.
   */
  finalizeCurrentRequest(): void {
    this.tokenUsageTracker.finalizeCurrentRequest();
  }

  // ============================================================
  // Rewrite the parent class method
  // ============================================================

  /**
   * Add a single message (override to invalidate cache)
   * @param message Message to add
   * @returns New message count
   */
  override addMessage(message: LLMMessage): number {
    const newIndex = super.addMessage(message);
    
    // Clear state from this point forward
    this.clearStateFromIndex(newIndex - 1);
    
    return newIndex;
  }

  /**
   * Clone a ConversationSession instance
   * @returns The cloned ConversationSession instance
   */
  override clone(): ConversationSession {
    const cloned = new ConversationSession({
      eventManager: this.eventManager,
      executionId: this.executionId,
      workflowId: this.workflowId,
      tokenLimit: this.tokenUsageTracker["tokenLimit"],
    });
    cloned.initializeFromHistory(this);
    return cloned;
  }

  /**
   * Initialize history from another manager
   * @param other: Another manager
   */
  initializeFromHistory(other: ConversationSession): void {
    this.initializeManagedMessages(other.getAllMessages());
    this.setMarkMap(other.getMarkMap());
  }

  /**
   * Initialize Manager Messages
   * @param messages List of messages
   */
  private initializeManagedMessages(messages: LLMMessage[]): void {
    super.clear();
    for (const msg of messages) {
      super.addMessage(msg);
    }
  }

  /**
   * Clean up resources
   */
  cleanup(): void {
    super.clear();
    this.tokenUsageTracker = new TokenUsageTracker({
      tokenLimit: this.tokenUsageTracker["tokenLimit"],
    });
    // Clear all turn states
    this.clearAllStates();
  }

  /**
   * Get the number of messages
   * @returns Count of messages
   */
  size(): number {
    return this.getAllMessages().length;
  }

  /**
   * Check if there are no messages
   * @returns true if empty
   */
  isEmpty(): boolean {
    return this.getAllMessages().length === 0;
  }

  // ============================================================
  // Turn-Based Execution State Methods (Persistent)
  // ============================================================

  /**
   * Get execution state for a specific turn.
   * This is part of the persistent state, not a transient cache.
   * @param turnStartIndex Index of the turn-start user message
   * @param key The state key (e.g., 'dynamicContext')
   * @returns State value or undefined if not found
   */
  getTurnState<T = unknown>(turnStartIndex: number, key: string): T | undefined {
    const turnState = this.turnStates.get(turnStartIndex);
    return turnState ? (turnState[key] as T) : undefined;
  }

  /**
   * Set execution state for a specific turn.
   * This state is persisted as part of the conversation history.
   * @param turnStartIndex Index of the turn-start user message
   * @param key The state key
   * @param value The state value
   */
  setTurnState(turnStartIndex: number, key: string, value: unknown): void {
    if (!this.turnStates.has(turnStartIndex)) {
      this.turnStates.set(turnStartIndex, {});
    }
    const turnState = this.turnStates.get(turnStartIndex)!;
    turnState[key] = value;
    
    logger.debug("Turn state updated", {
      turnStartIndex,
      key,
    });
  }

  /**
   * Clear state from a specific index onwards.
   * Used when editing or deleting messages to maintain consistency.
   * @param index Message index to clear from
   */
  clearStateFromIndex(index: number): void {
    const clearedKeys: number[] = [];

    for (const [key] of this.turnStates) {
      if (key >= index) {
        this.turnStates.delete(key);
        clearedKeys.push(key);
      }
    }

    if (clearedKeys.length > 0) {
      logger.debug("Cleared turn states", {
        fromIndex: index,
        clearedCount: clearedKeys.length,
        clearedIndices: clearedKeys,
      });
    }
  }

  /**
   * Clear all turn states.
   * Used when resetting conversation.
   */
  clearAllStates(): void {
    const clearedCount = this.turnStates.size;
    this.turnStates.clear();

    if (clearedCount > 0) {
      logger.debug("Cleared all turn states", { clearedCount });
    }
  }

  /**
   * Get dynamic context for a specific turn (Legacy alias for getTurnState)
   * @param turnStartIndex Index of the turn-start user message
   * @returns Context text or undefined if not found
   */
  getTurnDynamicContext(turnStartIndex: number): string | undefined {
    return this.getTurnState<string>(turnStartIndex, 'dynamicContext');
  }

  /**
   * Set dynamic context for a specific turn (Legacy alias for setTurnState)
   * @param turnStartIndex Index of the turn-start user message
   * @param context Generated dynamic context text
   */
  setTurnDynamicContext(turnStartIndex: number, context: string): void {
    this.setTurnState(turnStartIndex, 'dynamicContext', context);
  }

  /**
   * Clear cached context from a specific index onwards (Legacy alias for clearStateFromIndex)
   * Used when editing or deleting messages
   * @param index Message index to clear from
   */
  clearTurnContextFromIndex(index: number): void {
    this.clearStateFromIndex(index);
  }

  /**
   * Clear all cached turn contexts (Legacy alias for clearAllStates)
   * Used when resetting conversation
   */
  clearAllTurnContexts(): void {
    this.clearAllStates();
  }

  /**
   * Get conversation state for checkpointing
   * @returns Current conversation state
   */
  getState(): ConversationState {
    const baseState = this.createSnapshot();
    
    // Convert Map to plain object for serialization
    const serializedTurnStates: Record<number, Record<string, unknown>> = {};
    this.turnStates.forEach((value, key) => {
      serializedTurnStates[key] = value;
    });

    return {
      ...baseState,
      tokenUsage: this.tokenUsageTracker.getCumulativeUsage(),
      currentRequestUsage: this.tokenUsageTracker.getCurrentRequestUsage(),
      turnStates: serializedTurnStates,
    };
  }

  /**
   * Restore conversation state from checkpoint
   * @param state State to restore
   */
  restoreState(state: ConversationState): void {
    this.restoreFromSnapshot(state);
    
    if (state.tokenUsage) {
      this.tokenUsageTracker.setState(state.tokenUsage, state.currentRequestUsage);
    }

    // Restore turn states
    if (state.turnStates) {
      this.turnStates.clear();
      Object.entries(state.turnStates).forEach(([key, value]) => {
        this.turnStates.set(Number(key), value as Record<string, unknown>);
      });
      logger.debug("Restored turn states", { count: this.turnStates.size });
    }
  }
}
