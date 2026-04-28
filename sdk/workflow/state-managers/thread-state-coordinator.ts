/**
 * ThreadStateCoordinator - Thread State Coordinator
 * Unified management of WorkflowExecutionEntity and ConversationSession state
 *
 * Core Responsibilities:
 * - Coordinate message management between WorkflowExecutionEntity and ConversationSession
 * - Provide unified state snapshot and recovery interface
 * - Eliminate data redundancy and synchronization issues
 *
 * Design Principles:
 * - Single entry point for state management
 * - Clear responsibility boundaries
 * - Easy to extend for new state managers
 */

import type { LLMMessage, TokenUsageStats, MessageMarkMap } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "../entities/index.js";
import type { ConversationSession } from "../../core/messaging/conversation-session.js";

/**
 * Thread State Snapshot
 * Contains all state information needed for checkpoint
 */
export interface ThreadStateSnapshot {
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
 * ThreadStateCoordinator Configuration
 */
export interface ThreadStateCoordinatorConfig {
  /** Thread entity (required) */
  threadEntity: WorkflowExecutionEntity;
  /** Conversation session (required) */
  conversationManager: ConversationSession;
}

/**
 * ThreadStateCoordinator
 *
 * Coordinates state management between WorkflowExecutionEntity and ConversationSession.
 * This class provides a unified interface for message management and state operations,
 * eliminating the data redundancy and synchronization issues that existed when
 * WorkflowExecutionEntity directly held a ConversationSession.
 *
 * Usage:
 * - Created by execution layer (NodeExecutionCoordinator, ThreadBuilder, etc.)
 * - Provides unified message management interface
 * - Handles state snapshot and recovery for checkpoint mechanism
 */
export class ThreadStateCoordinator {
  private threadEntity: WorkflowExecutionEntity;
  private conversationManager: ConversationSession;

  constructor(config: ThreadStateCoordinatorConfig) {
    this.threadEntity = config.threadEntity;
    this.conversationManager = config.conversationManager;
  }

  // ============================================================
  // Message Management
  // ============================================================

  /**
   * Add a message to both managers
   * This replaces the previous dual-write pattern in WorkflowExecutionEntity
   * @param message LLM message
   */
  addMessage(message: LLMMessage): void {
    // Add to WorkflowExecutionEntity's message history
    this.threadEntity.messageHistoryManager.addMessage(message);
    // Sync to ConversationSession for Graph-specific features
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
   * @returns Array of visible messages
   */
  getMessages(): LLMMessage[] {
    return this.threadEntity.messageHistoryManager.getMessages();
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
    return this.threadEntity.messageHistoryManager.getRecentMessages(count);
  }

  /**
   * Set message history
   * @param messages Array of messages
   */
  setMessages(messages: LLMMessage[]): void {
    this.threadEntity.messageHistoryManager.setMessages(messages);
    this.conversationManager.clear();
    this.conversationManager.addMessages(...messages);
  }

  /**
   * Clear message history
   */
  clearMessages(): void {
    this.threadEntity.messageHistoryManager.clearMessages();
    this.conversationManager.clear();
  }

  /**
   * Normalize message history
   */
  normalizeHistory(): void {
    this.threadEntity.messageHistoryManager.normalizeHistory();
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
   * @returns Thread state snapshot
   */
  createSnapshot(): ThreadStateSnapshot {
    return {
      messages: this.conversationManager.getAllMessages(),
      markMap: this.conversationManager.getMarkMap(),
      tokenUsage: this.conversationManager.getTokenUsage(),
      currentRequestUsage: this.conversationManager.getCurrentRequestUsage(),
    };
  }

  /**
   * Restore from snapshot
   * @param snapshot Thread state snapshot
   */
  restoreFromSnapshot(snapshot: ThreadStateSnapshot): void {
    // Clear existing state
    this.threadEntity.messageHistoryManager.clearMessages();
    this.conversationManager.clear();

    // Restore messages
    if (snapshot.messages && snapshot.messages.length > 0) {
      for (const message of snapshot.messages) {
        this.threadEntity.messageHistoryManager.addMessage(message);
        this.conversationManager.addMessage(message);
      }
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
    this.threadEntity.messageHistoryManager.cleanup();
    this.conversationManager.cleanup();
  }

  // ============================================================
  // Accessors
  // ============================================================

  /**
   * Get the thread entity
   * @returns Thread entity
   */
  getThreadEntity(): WorkflowExecutionEntity {
    return this.threadEntity;
  }

  /**
   * Get the conversation manager
   * @returns Conversation session
   */
  getConversationManager(): ConversationSession {
    return this.conversationManager;
  }
}
