/**
 * AgentStateCoordinator - Agent State Coordinator
 *
 * Unified management of ConversationSession state for Agent Loops.
 * Extends BaseStateCoordinator for shared message management logic.
 * Modeled after WorkflowStateCoordinator for consistency.
 *
 * Core Responsibilities:
 * - Inherit common message/token/batch management from BaseStateCoordinator
 * - No additional agent-specific methods currently
 *
 * Design Principles:
 * - Single data source for message management
 * - Clear responsibility boundaries
 * - Checkpoint-compatible state serialization
 */

import {
  BaseStateCoordinator,
  type StateCoordinatorSnapshot,
  type BaseStateCoordinatorConfig,
} from "../../shared/messaging/base-state-coordinator.js";

/**
 * Agent State Snapshot
 * Alias for the common StateCoordinatorSnapshot type
 */
export type AgentStateSnapshot = StateCoordinatorSnapshot;

/**
 * AgentStateCoordinator Configuration
 * Alias for the common BaseStateCoordinatorConfig type
 */
export type AgentStateCoordinatorConfig = BaseStateCoordinatorConfig;

/**
 * AgentStateCoordinator
 *
 * Coordinates state management for ConversationSession.
 * Extends BaseStateCoordinator for unified message management interface.
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
export class AgentStateCoordinator extends BaseStateCoordinator<AgentStateSnapshot> {
  constructor(config: AgentStateCoordinatorConfig) {
    super(config);
  }
}
