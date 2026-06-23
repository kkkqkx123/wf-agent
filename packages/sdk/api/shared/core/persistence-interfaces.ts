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
import type { ResourceUsageRecord, IterationSystemMetrics, IterationLLMMetrics } from "../../../api/agent/resources/agent-loop-iteration-api.js";
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
   * Save resource usage record (legacy, for backward compatibility)
   */
  saveResourceUsageRecord(executionId: ID, record: ResourceUsageRecord): Promise<void>;

  /**
   * Get resource usage records with optional filter
   */
  getResourceUsageRecords(
    executionId: ID,
    filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
  ): Promise<ResourceUsageRecord[]>;

  /**
   * Save system metrics record (new method for Design Issue #4 fix)
   */
  saveSystemMetrics(executionId: ID, iteration: number, metrics: IterationSystemMetrics): Promise<void>;

  /**
   * Save LLM metrics records (new method for Design Issue #4 fix)
   */
  saveLLMMetrics(executionId: ID, metrics: IterationLLMMetrics[]): Promise<void>;

  /**
   * Get system metrics records
   */
  getSystemMetrics(
    executionId: ID,
    filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
  ): Promise<IterationSystemMetrics[]>;

  /**
   * Get LLM metrics records
   */
  getLLMMetrics(
    executionId: ID,
    filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
  ): Promise<IterationLLMMetrics[]>;

  // ============ Event history ============

  /**
   * Save event
   * @param executionId Execution ID to associate with the event
   * @param event Event record to persist
   */
  saveEvent(executionId: ID, event: ExecutionEventRecord): Promise<void>;

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

  async saveSystemMetrics(): Promise<void> {
    // No-op
  }

  async saveLLMMetrics(): Promise<void> {
    // No-op
  }

  async getSystemMetrics(): Promise<IterationSystemMetrics[]> {
    return [];
  }

  async getLLMMetrics(): Promise<IterationLLMMetrics[]> {
    return [];
  }

  async saveEvent(_executionId: ID, _event: ExecutionEventRecord): Promise<void> {
    // No-op
  }

  async queryEvents(_filter: EventQueryFilter): Promise<ExecutionEventRecord[]> {
    return [];
  }

  async countEvents(_filter: EventQueryFilter): Promise<number> {
    return 0;
  }

  async saveCheckpoint(_checkpointId: string, _metadata: Record<string, unknown>): Promise<void> {
    // No-op
  }

  async getCheckpointHistory(_executionId: ID): Promise<Array<{ checkpointId: string; timestamp: number }>> {
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
