/**
 * AgentStateCoordinator - Agent State Coordinator
 *
 * Unified management of ConversationSession state for Agent Loops.
 * Modeled after WorkflowStateCoordinator for consistency.
 *
 * Core Responsibilities:
 * - Manage ConversationSession for message operations
 * - Provide unified state snapshot and recovery interface
 * - Support parent-child execution message passing
 *
 * Design Principles:
 * - Single data source for message management
 * - Clear responsibility boundaries
 * - Checkpoint-compatible state serialization
 */

import type { LLMMessage, TokenUsageStats, MessageMarkMap } from "@wf-agent/types";
import type { ConversationSession } from "../../core/messaging/conversation-session.js";
import { RuntimeValidationError } from "@wf-agent/types";

/**
 * Agent State Snapshot
 * Contains all state information needed for checkpoint
 */
export interface AgentStateSnapshot {
  /** Message history state */
  messages: LLMMessage[];
  /** Message mark map for batch visibility */
  markMap: MessageMarkMap;
  /** Token usage statistics */
  tokenUsage?: TokenUsageStats;
  /** Current request token usage */
  currentRequestUsage?: TokenUsageStats;
}

/**
 * AgentStateCoordinator Configuration
 */
export interface AgentStateCoordinatorConfig {
  /** Conversation session (required) */
  conversationManager: ConversationSession;
}

/**
 * AgentStateCoordinator
 *
 * Coordinates state management for ConversationSession.
 * This class provides a unified interface for message management and state operations.
 *
 * Architecture Improvement (Eliminated Dual-Write):
 * - Previous: Messages stored in entity's internal ConversationSession
 * - Current: Messages managed by this coordinator (single data source)
 *
 * Usage:
 * - Created by AgentLoopCoordinator during entity creation
 * - Stored in AgentLoopRegistry alongside the entity
 * - Provides unified message management interface
 * - Handles state snapshot and recovery for checkpoint mechanism
 */
export class AgentStateCoordinator {
  private conversationManager: ConversationSession;

  constructor(config: AgentStateCoordinatorConfig) {
    this.conversationManager = config.conversationManager;
  }

  // ============================================================
  // Message Management (Single Data Source: ConversationSession)
  // ============================================================

  /**
   * Add a message to conversation
   * Uses ConversationSession as the single data source
   * @param message LLM message
   */
  addMessage(message: LLMMessage): void {
    if (!message) {
      throw new RuntimeValidationError("Message cannot be null", {
        operation: "addMessage",
        field: "message",
      });
    }

    if (!message.role || !message.content) {
      throw new RuntimeValidationError("Message must have role and content", {
        operation: "addMessage",
        field: "message",
      });
    }

    this.conversationManager.addMessage(message);
  }

  /**
   * Add multiple messages
   * @param messages Array of messages
   */
  addMessages(...messages: LLMMessage[]): void {
    for (const message of messages) {
      this.addMessage(message);
    }
  }

  /**
   * Get visible messages (for LLM)
   * Uses ConversationSession as the single data source
   * @returns Array of visible messages
   */
  getMessages(): LLMMessage[] {
    return this.conversationManager.getMessages();
  }

  /**
   * Get all messages (including invisible ones)
   * @returns Array of all messages
   */
  getAllMessages(): LLMMessage[] {
    return this.conversationManager.getAllMessages();
  }

  /**
   * Get recent messages
   * @param count Number of messages
   * @returns Array of recent messages
   */
  getRecentMessages(count: number): LLMMessage[] {
    return this.conversationManager.getRecentMessages(count);
  }

  /**
   * Get message count
   * @returns Number of messages
   */
  getMessageCount(): number {
    return this.conversationManager.getMessageCount();
  }

  /**
   * Set message history
   * Uses ConversationSession as the single data source
   * @param messages Array of messages
   */
  setMessages(messages: LLMMessage[]): void {
    this.conversationManager.clear();
    this.conversationManager.addMessages(...messages);
  }

  /**
   * Clear message history
   */
  clearMessages(): void {
    this.conversationManager.clear();
  }

  /**
   * Normalize message history
   */
  normalizeHistory(): void {
    // ConversationSession handles normalization internally
  }

  // ============================================================
  // Token Management (delegated to ConversationSession)
  // ============================================================

  /**
   * Get token usage statistics
   * @returns Token usage stats
   */
  getTokenUsage(): TokenUsageStats {
    return this.conversationManager.getTokenUsage();
  }

  /**
   * Get current request token usage
   * @returns Current request token usage
   */
  getCurrentRequestUsage(): TokenUsageStats {
    return this.conversationManager.getCurrentRequestUsage();
  }

  /**
   * Set token usage state (for checkpoint recovery)
   * @param cumulativeUsage Cumulative token usage
   * @param currentRequestUsage Current request token usage
   */
  setTokenUsageState(
    cumulativeUsage: TokenUsageStats | null,
    currentRequestUsage?: TokenUsageStats | null,
  ): void {
    this.conversationManager.setTokenUsageState(cumulativeUsage, currentRequestUsage);
  }

  /**
   * Check token usage and trigger events if needed
   */
  async checkTokenUsage(): Promise<void> {
    await this.conversationManager.checkTokenUsage();
  }

  // ============================================================
  // Batch Management (delegated to ConversationSession)
  // ============================================================

  /**
   * Get mark map
   * @returns Message mark map
   */
  getMarkMap(): MessageMarkMap {
    return this.conversationManager.getMarkMap();
  }

  /**
   * Set mark map
   * @param markMap Message mark map
   */
  setMarkMap(markMap: MessageMarkMap): void {
    this.conversationManager.setMarkMap(markMap);
  }

  /**
   * Start a new batch
   * @param boundaryIndex Boundary index
   * @returns New batch number
   */
  startNewBatch(boundaryIndex?: number): number {
    return this.conversationManager.startNewBatch(boundaryIndex);
  }

  /**
   * Start a new batch with auto checkpoint
   * @param boundaryIndex Boundary index
   * @param keepInMemory Number of batches to keep in memory
   * @returns New batch number
   */
  async startNewBatchWithAutoCheckpoint(
    boundaryIndex?: number,
    keepInMemory?: number,
  ): Promise<number> {
    return await this.conversationManager.startNewBatchWithAutoCheckpoint(
      boundaryIndex,
      keepInMemory,
    );
  }

  // ============================================================
  // State Snapshot & Recovery
  // ============================================================

  /**
   * Create a state snapshot
   * Uses ConversationSession as the single data source
   * @returns Agent state snapshot
   */
  createSnapshot(): AgentStateSnapshot {
    return {
      messages: this.conversationManager.getAllMessages(),
      markMap: this.conversationManager.getMarkMap(),
      tokenUsage: this.conversationManager.getTokenUsage(),
      currentRequestUsage: this.conversationManager.getCurrentRequestUsage(),
    };
  }

  /**
   * Restore from snapshot
   * Uses ConversationSession as the single data source
   * @param snapshot Agent state snapshot
   */
  restoreFromSnapshot(snapshot: AgentStateSnapshot): void {
    this.conversationManager.clear();

    if (snapshot.messages && snapshot.messages.length > 0) {
      this.conversationManager.addMessages(...snapshot.messages);
    }

    if (snapshot.markMap) {
      this.conversationManager.setMarkMap(snapshot.markMap);
    }

    if (snapshot.tokenUsage) {
      this.conversationManager.setTokenUsageState(
        snapshot.tokenUsage,
        snapshot.currentRequestUsage,
      );
    }
  }

  // ============================================================
  // Resource Management
  // ============================================================

  /**
   * Cleanup all resources
   */
  cleanup(): void {
    this.conversationManager.cleanup();
  }

  // ============================================================
  // Accessors
  // ============================================================

  /**
   * Get the conversation manager
   * @returns Conversation session
   */
  getConversationManager(): ConversationSession {
    return this.conversationManager;
  }
}
