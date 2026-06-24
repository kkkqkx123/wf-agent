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
import type { IterationSystemMetrics, IterationLLMMetrics } from "../../../api/agent/resources/agent-loop-iteration-api.js";
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
   * Save system metrics record
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
