/**
 * Persistence Layer Interfaces
 *
 * Design principles:
 * - Async interface supporting various backend storage
 * - Separation of concerns: persistence logic independent
 * - Optional features: applications can choose not to implement all interfaces
 */

import type { ID } from "@wf-agent/types";
import type { AgentExecutionState } from "../../../api/agent/resources/agent-execution-state-api.js";
import type { ResourceUsageRecord } from "../../../api/agent/resources/agent-loop-iteration-api.js";
import type { ExecutionEventRecord } from "@wf-agent/types";

/**
 * Time range filter
 */
export interface TimeRange {
  start: number;
  end: number;
}

/**
 * Event query filter
 */
export interface EventQueryFilter {
  executionId?: ID;
  workflowId?: ID;
  type?: string[];
  timeRange?: TimeRange;
  severity?: string[];
  limit?: number;
  offset?: number;
}

/**
 * Persistence layer health status
 */
export interface PersistenceLayerHealth {
  status: "healthy" | "degraded" | "unhealthy";
  storageHealth: "available" | "limited" | "unavailable";
  lastFlush?: number;
  pendingWrites?: number;
  message?: string;
}

/**
 * Unified persistence layer interface
 *
 * Provides comprehensive persistence capabilities for execution states,
 * performance metrics, events, and checkpoints.
 */
export interface PersistenceLayer {
  // ============ Execution state snapshots ============

  /**
   * Save execution state snapshot
   */
  saveExecutionStateSnapshot(snapshot: AgentExecutionState): Promise<void>;

  /**
   * Get execution state snapshot at a specific iteration
   * @param executionId Execution ID
   * @param iteration Optional iteration number
   */
  getExecutionStateSnapshot(executionId: ID, iteration?: number): Promise<AgentExecutionState | null>;

  /**
   * List execution state snapshots for an execution
   */
  listExecutionStateSnapshots(executionId: ID, limit?: number): Promise<AgentExecutionState[]>;

  // ============ Performance metrics ============

  /**
   * Save resource usage record
   */
  saveResourceUsageRecord(executionId: ID, record: ResourceUsageRecord): Promise<void>;

  /**
   * Get resource usage records with optional filter
   */
  getResourceUsageRecords(
    executionId: ID,
    filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
  ): Promise<ResourceUsageRecord[]>;

  // ============ Event history ============

  /**
   * Save event
   */
  saveEvent(event: ExecutionEventRecord): Promise<void>;

  /**
   * Query events with filter
   */
  queryEvents(filter: EventQueryFilter): Promise<ExecutionEventRecord[]>;

  /**
   * Count events matching filter
   */
  countEvents(filter: EventQueryFilter): Promise<number>;

  // ============ Checkpoints ============

  /**
   * Save checkpoint metadata
   */
  saveCheckpoint(checkpointId: string, metadata: Record<string, unknown>): Promise<void>;

  /**
   * Get checkpoint history
   */
  getCheckpointHistory(executionId: ID): Promise<Array<{ checkpointId: string; timestamp: number }>>;

  // ============ Lifecycle ============

  /**
   * Initialize persistence layer
   */
  initialize(): Promise<void>;

  /**
   * Shutdown persistence layer (flush pending writes)
   */
  shutdown(): Promise<void>;

  /**
   * Check health status
   */
  health(): Promise<PersistenceLayerHealth>;
}

/**
 * No-op persistence layer implementation
 * Used for testing and when persistence is disabled
 */
export class NoOpPersistenceLayer implements PersistenceLayer {
  async saveExecutionStateSnapshot(): Promise<void> {
    // No-op
  }

  async getExecutionStateSnapshot(): Promise<null> {
    return null;
  }

  async listExecutionStateSnapshots(): Promise<AgentExecutionState[]> {
    return [];
  }

  async saveResourceUsageRecord(): Promise<void> {
    // No-op
  }

  async getResourceUsageRecords(): Promise<ResourceUsageRecord[]> {
    return [];
  }

  async saveEvent(): Promise<void> {
    // No-op
  }

  async queryEvents(): Promise<ExecutionEventRecord[]> {
    return [];
  }

  async countEvents(): Promise<number> {
    return 0;
  }

  async saveCheckpoint(): Promise<void> {
    // No-op
  }

  async getCheckpointHistory(): Promise<Array<{ checkpointId: string; timestamp: number }>> {
    return [];
  }

  async initialize(): Promise<void> {
    // No-op
  }

  async shutdown(): Promise<void> {
    // No-op
  }

  async health(): Promise<PersistenceLayerHealth> {
    return { status: "healthy", storageHealth: "available" };
  }
}
