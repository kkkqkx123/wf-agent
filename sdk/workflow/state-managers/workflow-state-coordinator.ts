/**
 * WorkflowStateCoordinator - Workflow State Coordinator
 *
 * Unified management of ConversationSession state.
 * Extends BaseStateCoordinator for shared message management logic,
 * with additional parent-child execution message passing methods.
 *
 * Core Responsibilities:
 * - Inherit common message/token/batch management from BaseStateCoordinator
 * - Support parent-child execution message passing (fork, subgraph, triggered workflow)
 *
 * Architecture Design:
 * - All message operations: Only use ConversationSession (single data source)
 * - Parent-child message passing: Use export/import methods
 *
 * Design Principles:
 * - Single data source for message management
 * - Clear responsibility boundaries
 */

import type { LLMMessage } from "@wf-agent/types";
import {
  BaseStateCoordinator,
  type StateCoordinatorSnapshot,
  type BaseStateCoordinatorConfig,
} from "../../core/messaging/base-state-coordinator.js";

/**
 * Workflow State Snapshot
 * Alias for the common StateCoordinatorSnapshot type
 */
export type WorkflowStateSnapshot = StateCoordinatorSnapshot;

/**
 * WorkflowStateCoordinator Configuration
 * Alias for the common BaseStateCoordinatorConfig type
 */
export type WorkflowStateCoordinatorConfig = BaseStateCoordinatorConfig;

/**
 * WorkflowStateCoordinator
 *
 * Coordinates state management for ConversationSession.
 * Extends BaseStateCoordinator with workflow-specific parent-child message passing.
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
export class WorkflowStateCoordinator extends BaseStateCoordinator<WorkflowStateSnapshot> {
  constructor(config: WorkflowStateCoordinatorConfig) {
    super(config);
  }

  // ============================================================
  // Parent-Child Execution Message Passing (Workflow-specific)
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
}
