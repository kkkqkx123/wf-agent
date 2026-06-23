/**
 * Agent Loop Entity Storage Adapter Interface Definition
 * Defines a uniform interface for agent loop lifecycle persistence operations
 */

import type {
  AgentEntityMetadata,
  AgentEntityListOptions
} from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";
import type { AgentLoopStatus } from "@wf-agent/types";

/**
 * Agent Loop Entity Storage Adapter Interface
 *
 * Unified interface for managing agent loop lifecycle persistence
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - Adds agent-loop-specific methods for status management and querying.
 * - packages/storage provides implementations based on this interface.
 */
export interface AgentLoopStorageAdapter 
  extends BaseStorageAdapter<AgentEntityMetadata, AgentEntityListOptions> {
  
  /**
   * Update agent loop status
   * @param agentLoopId The agent loop ID
   * @param status New status to set
   */
  updateAgentLoopStatus(agentLoopId: string, status: AgentLoopStatus): Promise<void>;
  
  /**
   * List agent loops by status
   * @param status Filter by status
   * @returns Array of agent loop IDs
   */
  listByStatus(status: AgentLoopStatus): Promise<string[]>;
  
  /**
   * Get agent loop statistics
   * @returns Statistics about agent loops grouped by status
   */
  getAgentLoopStats(): Promise<{
    total: number;
    byStatus: Record<string, number>;
  }>;
}
