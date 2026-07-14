/**
 * Agent Execution Registry - Interface and Types
 *
 * Defines the contract for querying agent execution data.
 * Used by triggers and other components to access execution data
 * without coupling to concrete implementations.
 */

import type { ID } from "@wf-agent/types";
import type { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import type { AgentStateCoordinator } from "../state-managers/agent-state-coordinator.js";

/**
 * Filter for querying agent executions
 */
export interface AgentExecutionFilter {
  /** Filter by status */
  status?: string;
  /** Filter by parent workflow execution ID */
  parentWorkflowId?: string;
}

/**
 * Agent Execution Registry Interface
 *
 * Provides unified access to agent execution instances.
 * Implementations manage lifecycle and querying of AgentLoopEntity instances.
 */
export interface IAgentExecutionRegistry {
  /**
   * Register an agent loop entity
   */
  register(entity: AgentLoopEntity): void;

  /**
   * Get an agent loop entity by ID
   */
  get(id: ID): Promise<AgentLoopEntity | undefined>;

  /**
   * Unregister an agent loop entity
   */
  unregister(id: ID): boolean;

  /**
   * Check if an entity exists
   */
  has(id: ID): boolean;

  /**
   * Query agent loop entities with optional filter
   */
  query(filter?: AgentExecutionFilter): AgentLoopEntity[];

  /**
   * Get the AgentStateCoordinator for a given agent loop entity
   * @param agentLoopId Agent Loop ID
   * @returns AgentStateCoordinator instance or null
   */
  getStateCoordinator(agentLoopId: ID): AgentStateCoordinator | null;
}