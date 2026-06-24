/**
 * No-Op Persistence Layer
 *
 * Implements PersistenceLayer interface with no-op methods.
 * Used for testing and scenarios where persistence is not needed.
 *
 * WARNING: This should only be used in testing. Using this in production
 * will result in loss of all execution data.
 */

import type { PersistenceLayer, PersistenceLayerHealth, EventQueryFilter, TimeRange } from "../persistence-interfaces.js";
import type { ID } from "@wf-agent/types";
import type { AgentExecutionState } from "../../../agent/resources/agent-execution-state-api.js";
import type { IterationSystemMetrics, IterationLLMMetrics } from "../../../agent/resources/agent-loop-iteration-api.js";
import type { ExecutionEventRecord } from "@wf-agent/types";

/**
 * No-Op Persistence Layer Implementation
 *
 * All methods are implemented as no-ops (they do nothing).
 * This is useful for testing and scenarios where you don't need persistence.
 */
export class NoOpPersistenceLayer implements PersistenceLayer {
  async saveExecutionStateSnapshot(_snapshot: AgentExecutionState): Promise<void> {
    // no-op
  }

  async getExecutionStateSnapshot(
    _executionId: ID,
    _iteration?: number,
  ): Promise<AgentExecutionState | null> {
    return null;
  }

  async listExecutionStateSnapshots(_executionId: ID, _limit?: number): Promise<AgentExecutionState[]> {
    return [];
  }

  async saveSystemMetrics(
    _executionId: ID,
    _iteration: number,
    _metrics: IterationSystemMetrics,
  ): Promise<void> {
    // no-op
  }

  async saveLLMMetrics(_executionId: ID, _metrics: IterationLLMMetrics[]): Promise<void> {
    // no-op
  }

  async getSystemMetrics(
    _executionId: ID,
    _filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
  ): Promise<IterationSystemMetrics[]> {
    return [];
  }

  async getLLMMetrics(
    _executionId: ID,
    _filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
  ): Promise<IterationLLMMetrics[]> {
    return [];
  }

  async saveEvent(_executionId: ID, _event: ExecutionEventRecord): Promise<void> {
    // no-op
  }

  async queryEvents(_filter: EventQueryFilter): Promise<ExecutionEventRecord[]> {
    return [];
  }

  async countEvents(_filter: EventQueryFilter): Promise<number> {
    return 0;
  }

  async saveCheckpoint(_checkpointId: string, _metadata: Record<string, unknown>): Promise<void> {
    // no-op
  }

  async getCheckpointHistory(_executionId: ID): Promise<Array<{ checkpointId: string; timestamp: number }>> {
    return [];
  }

  async initialize(): Promise<void> {
    // no-op
  }

  async shutdown(): Promise<void> {
    // no-op
  }

  async health(): Promise<PersistenceLayerHealth> {
    return {
      status: "healthy",
      storageHealth: "available",
      message: "No-Op persistence layer - no actual storage",
    };
  }
}
