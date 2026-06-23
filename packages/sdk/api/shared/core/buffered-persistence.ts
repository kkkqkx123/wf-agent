/**
 * Buffered Persistence Layer
 *
 * Strategy:
 * 1. Maintain in-memory buffers for recent records
 * 2. Trigger async flush on buffer full or time interval
 * 3. Write to database
 * 4. Prioritize buffer queries, then database
 */

import type {
  PersistenceLayer,
  EventQueryFilter,
  PersistenceLayerHealth,
} from "./persistence-interfaces.js";
import { NoOpPersistenceLayer } from "./persistence-interfaces.js";
import type { ID } from "@wf-agent/types";
import type { AgentExecutionState } from "../../agent/resources/agent-execution-state-api.js";
import type { ResourceUsageRecord } from "../../agent/resources/agent-loop-iteration-api.js";
import type { ExecutionEventRecord } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "BufferedPersistenceLayer" });

/**
 * Buffered persistence layer implementation
 *
 * Provides efficient persistence with in-memory caching and async flushing
 */
export class BufferedPersistenceLayer implements PersistenceLayer {
  private backend: PersistenceLayer;

  // Buffers
  private stateBuffer: Map<ID, AgentExecutionState[]> = new Map();
  private metricsBuffer: Map<ID, ResourceUsageRecord[]> = new Map();
  private eventBuffer: ExecutionEventRecord[] = [];

  // Configuration
  private stateBufferSize: number = 50;
  private metricsBufferSize: number = 200;
  private eventBufferSize: number = 1000;
  private flushIntervalMs: number = 30000; // 30 seconds

  // State
  private flushTimer: NodeJS.Timeout | null = null;
  private isFlushing: boolean = false;
  private isShuttingDown: boolean = false;

  /**
   * Constructor
   * @param backend Actual persistence backend
   */
  constructor(backend?: PersistenceLayer) {
    this.backend = backend || new NoOpPersistenceLayer();
  }

  async initialize(): Promise<void> {
    await this.backend.initialize();
    this.startAutoFlush();
    logger.info("BufferedPersistenceLayer initialized");
  }

  async shutdown(): Promise<void> {
    this.isShuttingDown = true;

    // Final flush
    await this.flush();

    if (this.flushTimer) {
      clearInterval(this.flushTimer);
    }

    await this.backend.shutdown();
    logger.info("BufferedPersistenceLayer shutdown");
  }

  // ============ Execution state snapshots ============

  async saveExecutionStateSnapshot(snapshot: AgentExecutionState): Promise<void> {
    // Add to buffer
    const buffer = this.getOrCreateStateBuffer(snapshot.executionId);
    buffer.push(snapshot);

    // Flush if buffer full
    if (buffer.length >= this.stateBufferSize) {
      await this.flushStateBuffer(snapshot.executionId);
    }
  }

  async getExecutionStateSnapshot(
    executionId: ID,
    iteration?: number,
  ): Promise<AgentExecutionState | null> {
    // Query buffer first
    const buffer = this.stateBuffer.get(executionId);
    if (buffer && buffer.length > 0) {
      if (!iteration) {
        return buffer[buffer.length - 1] ?? null;
      }
      return buffer.find((s) => s.currentIteration === iteration) ?? null;
    }

    // Query backend
    return await this.backend.getExecutionStateSnapshot(executionId, iteration);
  }

  async listExecutionStateSnapshots(
    executionId: ID,
    limit?: number,
  ): Promise<AgentExecutionState[]> {
    const buffer = this.stateBuffer.get(executionId) ?? [];
    const backendList = await this.backend.listExecutionStateSnapshots(
      executionId,
      limit ? limit * 2 : undefined,
    );

    // Merge and deduplicate
    const combined = [...buffer, ...backendList];
    const seen = new Set<number>();

    return combined
      .reverse()
      .filter((s) => {
        if (seen.has(s.currentIteration)) return false;
        seen.add(s.currentIteration);
        return true;
      })
      .reverse()
      .slice(0, limit);
  }

  // ============ Performance metrics ============

  async saveResourceUsageRecord(executionId: ID, record: ResourceUsageRecord): Promise<void> {
    const buffer = this.getOrCreateMetricsBuffer(executionId);
    buffer.push(record);

    if (buffer.length >= this.metricsBufferSize) {
      await this.flushMetricsBuffer(executionId);
    }
  }

  async getResourceUsageRecords(
    executionId: ID,
    filter?: any,
  ): Promise<ResourceUsageRecord[]> {
    const buffer = this.metricsBuffer.get(executionId) ?? [];

    if (filter?.timeRange) {
      // Time range queries go to backend
      return await this.backend.getResourceUsageRecords(executionId, filter);
    }

    // Return recent records from buffer
    return buffer.slice(-100);
  }

  // ============ Event history ============

  async saveEvent(event: ExecutionEventRecord): Promise<void> {
    this.eventBuffer.push(event);

    if (this.eventBuffer.length >= this.eventBufferSize) {
      await this.flushEventBuffer();
    }
  }

  async queryEvents(filter: EventQueryFilter): Promise<ExecutionEventRecord[]> {
    // Query buffer
    let bufferResults = this.eventBuffer;

    if (filter.executionId) {
      bufferResults = bufferResults.filter((e) => (e as any).executionId === filter.executionId);
    }

    if (filter.type && filter.type.length > 0) {
      bufferResults = bufferResults.filter((e) => filter.type!.includes(e.type));
    }

    // Query backend
    const backendResults = await this.backend.queryEvents(filter);

    // Merge results
    const combined = [...bufferResults, ...backendResults];
    return combined.slice(0, filter.limit ?? 100);
  }

  async countEvents(filter: EventQueryFilter): Promise<number> {
    const results = await this.queryEvents(filter);
    return results.length;
  }

  // ============ Checkpoints ============

  async saveCheckpoint(checkpointId: string, metadata: Record<string, unknown>): Promise<void> {
    await this.backend.saveCheckpoint(checkpointId, metadata);
  }

  async getCheckpointHistory(
    executionId: ID,
  ): Promise<Array<{ checkpointId: string; timestamp: number }>> {
    return await this.backend.getCheckpointHistory(executionId);
  }

  // ============ Lifecycle ============

  async health(): Promise<PersistenceLayerHealth> {
    const backendHealth = await this.backend.health();
    return {
      ...backendHealth,
      pendingWrites:
        this.stateBuffer.size + this.metricsBuffer.size + this.eventBuffer.length,
    };
  }

  // ============ Internal methods ============

  private startAutoFlush(): void {
    this.flushTimer = setInterval(async () => {
      await this.flush();
    }, this.flushIntervalMs);
  }

  private async flush(): Promise<void> {
    if (this.isFlushing || this.isShuttingDown) return;

    this.isFlushing = true;
    try {
      // Flush all buffers
      const stateIds = Array.from(this.stateBuffer.keys());
      for (const id of stateIds) {
        await this.flushStateBuffer(id);
      }

      const metricIds = Array.from(this.metricsBuffer.keys());
      for (const id of metricIds) {
        await this.flushMetricsBuffer(id);
      }

      if (this.eventBuffer.length > 0) {
        await this.flushEventBuffer();
      }

      logger.debug("Flush completed", {
        states: stateIds.length,
        metrics: metricIds.length,
        events: this.eventBuffer.length,
      });
    } catch (error) {
      logger.error("Flush failed", { error });
    } finally {
      this.isFlushing = false;
    }
  }

  private async flushStateBuffer(executionId: ID): Promise<void> {
    const buffer = this.stateBuffer.get(executionId);
    if (!buffer || buffer.length === 0) return;

    try {
      for (const snapshot of buffer) {
        await this.backend.saveExecutionStateSnapshot(snapshot);
      }
      this.stateBuffer.delete(executionId);
    } catch (error) {
      logger.error("Failed to flush state buffer", { executionId, error });
    }
  }

  private async flushMetricsBuffer(executionId: ID): Promise<void> {
    const buffer = this.metricsBuffer.get(executionId);
    if (!buffer || buffer.length === 0) return;

    try {
      for (const record of buffer) {
        await this.backend.saveResourceUsageRecord(executionId, record);
      }
      this.metricsBuffer.delete(executionId);
    } catch (error) {
      logger.error("Failed to flush metrics buffer", { executionId, error });
    }
  }

  private async flushEventBuffer(): Promise<void> {
    if (this.eventBuffer.length === 0) return;

    const events = this.eventBuffer.splice(0, this.eventBufferSize);

    try {
      for (const event of events) {
        await this.backend.saveEvent(event);
      }
    } catch (error) {
      logger.error("Failed to flush event buffer", { error });
      // Restore failed events to front of buffer for retry
      this.eventBuffer.unshift(...events);
    }
  }

  private getOrCreateStateBuffer(executionId: ID): AgentExecutionState[] {
    if (!this.stateBuffer.has(executionId)) {
      this.stateBuffer.set(executionId, []);
    }
    return this.stateBuffer.get(executionId)!;
  }

  private getOrCreateMetricsBuffer(executionId: ID): ResourceUsageRecord[] {
    if (!this.metricsBuffer.has(executionId)) {
      this.metricsBuffer.set(executionId, []);
    }
    return this.metricsBuffer.get(executionId)!;
  }
}
