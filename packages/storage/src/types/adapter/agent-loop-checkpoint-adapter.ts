/**
 * Agent Loop Checkpoint Storage Adapter Interface Definition
 * Defines a uniform interface for agent loop checkpoint persistence operations
 */

import type { 
  AgentLoopCheckpointStorageMetadata, 
  AgentLoopCheckpointStorageListOptions 
} from "@wf-agent/types";
import type { BaseStorageAdapter } from "./base-storage-adapter.js";

/**
 * Agent Loop Checkpoint Storage Adapter Interface
 *
 * Unified interface for defining agent loop checkpoint persistence operations
 * - Inherits from BaseStorageAdapter and provides standard CRUD operations.
 * - Adds agent-loop-specific methods for efficient querying and management.
 * - packages/storage provides implementations based on this interface.
 */
export interface AgentLoopCheckpointStorageAdapter 
  extends BaseStorageAdapter<AgentLoopCheckpointStorageMetadata, AgentLoopCheckpointStorageListOptions> {
  
  /**
   * List checkpoints for a specific agent loop
   * @param agentLoopId The agent loop ID to filter by
   * @param options Additional filtering options (excluding agentLoopId)
   * @returns Array of checkpoint IDs
   */
  listByAgentLoop(
    agentLoopId: string, 
    options?: Omit<AgentLoopCheckpointStorageListOptions, 'agentLoopId'>
  ): Promise<string[]>;
  
  /**
   * Get the latest checkpoint ID for an agent loop
   * @param agentLoopId The agent loop ID
   * @returns Latest checkpoint ID or null if none exists
   */
  getLatestCheckpoint(agentLoopId: string): Promise<string | null>;
  
  /**
   * Delete all checkpoints for an agent loop
   * @param agentLoopId The agent loop ID
   * @returns Number of deleted checkpoints
   */
  deleteByAgentLoop(agentLoopId: string): Promise<number>;
}
