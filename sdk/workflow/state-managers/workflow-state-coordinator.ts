/**
 * WorkflowStateCoordinator - Workflow State Coordinator
 * Unified management of ConversationSession state
 *
 * Core Responsibilities:
 * - Manage ConversationSession for message operations
 * - Provide unified state snapshot and recovery interface
 * - Support parent-child execution message passing
 *
 * Architecture Design:
 * - All message operations: Only use ConversationSession (single data source)
 * - Parent-child message passing: Use export/import methods
 *
 * Design Principles:
 * - Single data source for message management
 * - Clear responsibility boundaries
 */

import type { LLMMessage, TokenUsageStats, MessageMarkMap } from "@wf-agent/types";
import type { ConversationSession } from "../../core/messaging/conversation-session.js";
import { RuntimeValidationError } from "@wf-agent/types";

/**
 * Workflow State Snapshot
 * Contains all state information needed for checkpoint
 */
export interface WorkflowStateSnapshot {
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
 * WorkflowStateCoordinator Configuration
 */
export interface WorkflowStateCoordinatorConfig {
  /** Conversation session (required) */
  conversationManager: ConversationSession;
}

/**
 * WorkflowStateCoordinator
 *
 * Coordinates state management for ConversationSession.
 * This class provides a unified interface for message management and state operations.
 *
 * Architecture Improvement (Eliminated Dual-Write):
 * - Previous: Messages stored in both messageHistoryManager and conversationManager
 * - Current: Messages only stored in conversationManager (single data source)
 *
 * Usage:
 * - Created by execution layer (NodeExecutionCoordinator, WorkflowExecutionBuilder, etc.)
 * - Provides unified message management interface
 * - Handles state snapshot and recovery for checkpoint mechanism
 */
export class WorkflowStateCoordinator {
  private conversationManager: ConversationSession;

  constructor(config: WorkflowStateCoordinatorConfig) {
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
    // Validate input
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

    // Only add to ConversationSession (single data source)
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
    // Clear and set in ConversationSession (single data source)
    this.conversationManager.clear();
    this.conversationManager.addMessages(...messages);
  }

  /**
   * Clear message history
   * Uses ConversationSession as the single data source
   */
  clearMessages(): void {
    this.conversationManager.clear();
  }

  /**
   * Normalize message history
   * Uses ConversationSession as the single data source
   */
  normalizeHistory(): void {
    // ConversationSession handles normalization internally
    // Note: If needed, this can be delegated to conversationManager
    // Currently, MessageHistory.normalizeHistory() handles tool call completion
    // ConversationSession may have similar functionality
  }

  // ============================================================
  // Parent-Child Execution Message Passing
  // ============================================================

  /**
   * Export messages for child execution
   * Used when creating child executions (fork, subgraph, triggered workflow)
   * @returns Array of messages to pass to child execution
   */
  exportMessagesForChild(): LLMMessage[] {
    return this.conversationManager.getMessages();
  }

  /**
   * Import messages from child execution
   * Used when merging child execution results back to parent
   * @param messages Messages from child execution
   */
  importMessagesFromChild(messages: LLMMessage[]): void {
    this.conversationManager.addMessages(...messages);
  }

  /**
   * Export all messages (including invisible) for checkpoint
   * @returns Array of all messages
   */
  exportAllMessagesForCheckpoint(): LLMMessage[] {
    return this.conversationManager.getAllMessages();
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
   * @returns Workflow state snapshot
   */
  createSnapshot(): WorkflowStateSnapshot {
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
   * @param snapshot Workflow state snapshot
   */
  restoreFromSnapshot(snapshot: WorkflowStateSnapshot): void {
    // Clear existing state in ConversationSession
    this.conversationManager.clear();

    // Restore messages to ConversationSession (single data source)
    if (snapshot.messages && snapshot.messages.length > 0) {
      this.conversationManager.addMessages(...snapshot.messages);
    }

    // Restore mark map
    if (snapshot.markMap) {
      this.conversationManager.setMarkMap(snapshot.markMap);
    }

    // Restore token usage
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
    // Cleanup ConversationSession (primary data source)
    this.conversationManager.cleanup();

    // Note: messageHistoryManager cleanup is handled by WorkflowExecutionEntity
    // We don't directly manage it here to maintain clear responsibility boundaries
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
