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

import {
  BaseStateCoordinator,
  type StateCoordinatorSnapshot,
  type BaseStateCoordinatorConfig,
} from "../../shared/messaging/base-state-coordinator.js";

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
  // Parent-child message passing methods inherited from BaseStateCoordinator:
  // - exportMessagesForChild()
  // - importMessagesFromChild()
  // - exportAllMessagesForCheckpoint()
}
