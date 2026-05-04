/**
 * WorkflowStateCoordinator - Workflow State Coordinator
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
import type { WorkflowExecutionEntity } from "../entities/workflow-execution-entity.js";
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
  /** Workflow execution entity (required) */
  workflowExecutionEntity: WorkflowExecutionEntity;
  /** Conversation session (required) */
  conversationManager: ConversationSession;
}

/**
 * WorkflowStateCoordinator
 *
 * Coordinates state management between WorkflowExecutionEntity and ConversationSession.
 * This class provides a unified interface for message management and state operations,
 * eliminating the data redundancy and synchronization issues that existed when
 * WorkflowExecutionEntity directly held a ConversationSession.
 *
 * Usage:
 * - Created by execution layer (NodeExecutionCoordinator, WorkflowExecutionBuilder, etc.)
 * - Provides unified message management interface
 * - Handles state snapshot and recovery for checkpoint mechanism
 */
export class WorkflowStateCoordinator {
  private workflowExecutionEntity: WorkflowExecutionEntity;
  private conversationManager: ConversationSession;

  constructor(config: WorkflowStateCoordinatorConfig) {
    this.workflowExecutionEntity = config.workflowExecutionEntity;
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

    // Add to WorkflowExecutionEntity's message history
    this.workflowExecutionEntity.messageHistoryManager.addMessage(message);
    // Sync to ConversationSession for Workflow-specific features
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
    return this.workflowExecutionEntity.messageHistoryManager.getMessages();
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
    return this.workflowExecutionEntity.messageHistoryManager.getRecentMessages(count);
  }

  /**
   * Set message history
   * @param messages Array of messages
   */
  setMessages(messages: LLMMessage[]): void {
    this.workflowExecutionEntity.messageHistoryManager.setMessages(messages);
    this.conversationManager.clear();
    this.conversationManager.addMessages(...messages);
  }

  /**
   * Clear message history
   */
  clearMessages(): void {
    this.workflowExecutionEntity.messageHistoryManager.clearMessages();
    this.conversationManager.clear();
  }

  /**
   * Normalize message history
   */
  normalizeHistory(): void {
    this.workflowExecutionEntity.messageHistoryManager.normalizeHistory();
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
   * @param snapshot Workflow state snapshot
   */
  restoreFromSnapshot(snapshot: WorkflowStateSnapshot): void {
    // Clear existing state
    this.workflowExecutionEntity.messageHistoryManager.clearMessages();
    this.conversationManager.clear();

    // Restore messages
    if (snapshot.messages && snapshot.messages.length > 0) {
      for (const message of snapshot.messages) {
        this.workflowExecutionEntity.messageHistoryManager.addMessage(message);
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
    this.workflowExecutionEntity.messageHistoryManager.cleanup();
    this.conversationManager.cleanup();
  }

  // ============================================================
  // Accessors
  // ============================================================

  /**
   * Get the workflow execution entity
   * @returns Workflow execution entity
   */
  getWorkflowExecutionEntity(): WorkflowExecutionEntity {
    return this.workflowExecutionEntity;
  }

  /**
   * Get the conversation manager
   * @returns Conversation session
   */
  getConversationManager(): ConversationSession {
    return this.conversationManager;
  }
}
