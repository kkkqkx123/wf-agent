/**
 * SQLite Persistence Layer Implementation
 *
 * Provides persistent storage for execution events, states, and metrics
 * using SQLite database backend.
 *
 * Features:
 * - Unlimited event storage (solves circular buffer limitation)
 * - Automatic schema creation
 * - Async write queuing for performance
 * - TTL-based cleanup (configurable retention)
 *
 * Part of P0 priority: Fix event persistence gap
 */

import type { ID } from "@wf-agent/types";
import type { ExecutionEventRecord } from "@wf-agent/types";
import type { AgentExecutionState } from "../../../api/agent/resources/agent-execution-state-api.js";
import type { ResourceUsageRecord } from "../../../api/agent/resources/agent-loop-iteration-api.js";
import type {
  PersistenceLayer,
  PersistenceLayerHealth,
  EventQueryFilter,
  TimeRange,
} from "../core/persistence-interfaces.js";
import type { IterationSystemMetrics, IterationLLMMetrics } from "../../agent/resources/agent-loop-iteration-api.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import Database from "better-sqlite3";
import type { Database as DatabaseType } from "better-sqlite3";

const logger = createContextualLogger({ operation: "SQLitePersistenceLayer" });

/**
 * Internal type for persisted resource usage records
 * Matches the database schema, distinct from the public ResourceUsageRecord
 */

/**
 * SQLite Persistence Layer
 *
 * Configuration:
 * - path: Database file path (default: ':memory:' for in-memory, or './execution.db')
 * - retentionDays: How long to keep old events (default: 30 days)
 * - batchSize: Events to batch before writing (default: 100)
 * - flushInterval: Interval to flush writes (default: 5000ms)
 */
export class SQLitePersistenceLayer implements PersistenceLayer {
  private db: DatabaseType | null = null;
  private path: string;
  private retentionDays: number;
  private batchSize: number;
  private flushInterval: number;
  private eventQueue: Array<{ executionId: ID; event: ExecutionEventRecord }> = [];
  private flushTimer: NodeJS.Timeout | null = null;
  private isInitialized = false;
  private isShuttingDown = false;
  private lastFlushTime: number = 0;

  constructor(options?: {
    path?: string;
    retentionDays?: number;
    batchSize?: number;
    flushInterval?: number;
  }) {
    this.path = options?.path || ':memory:';
    this.retentionDays = options?.retentionDays || 30;
    this.batchSize = options?.batchSize || 100;
    this.flushInterval = options?.flushInterval || 5000;
  }

  /**
   * Initialize the persistence layer and create schema
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      return;
    }

    try {
      logger.info("Initializing SQLite persistence layer", { path: this.path });

      // Create actual database connection using better-sqlite3
      this.db = new Database(this.path);

      // Enable foreign keys and optimize for concurrent access
      this.db.pragma("journal_mode = WAL");
      this.db.pragma("synchronous = NORMAL");
      this.db.pragma("foreign_keys = ON");

      // Create schema
      this.createSchema();

      this.isInitialized = true;

      // Start flush timer
      this.startFlushTimer();

      logger.info("SQLite persistence layer initialized successfully", { path: this.path });
    } catch (error) {
      logger.error("Failed to initialize SQLite persistence layer", { error });
      throw error;
    }
  }

  /**
   * Shutdown and flush pending writes
   */
  async shutdown(): Promise<void> {
    if (this.isShuttingDown) {
      return;
    }

    this.isShuttingDown = true;

    try {
      logger.info("Shutting down SQLite persistence layer");

      // Stop flush timer
      if (this.flushTimer) {
        clearInterval(this.flushTimer);
        this.flushTimer = null;
      }

      // Flush any pending events
      if (this.eventQueue.length > 0) {
        await this.flushEvents();
      }

      // Clean up old records
      await this.cleanupOldRecords();

      // Close database connection
      if (this.db) {
        this.db.close();
        this.db = null;
      }

      logger.info("SQLite persistence layer shut down successfully");
    } catch (error) {
      logger.error("Error during shutdown", { error });
    }
  }

  /**
   * Get health status
   */
  async health(): Promise<PersistenceLayerHealth> {
    if (!this.isInitialized || this.isShuttingDown) {
      return {
        status: 'unhealthy',
        storageHealth: 'unavailable',
        message: 'Persistence layer not initialized or shutting down',
      };
    }

    return {
      status: 'healthy',
      storageHealth: 'available',
      lastFlush: this.getLastFlushTime(),
      pendingWrites: this.eventQueue.length,
    };
  }

  /**
   * Save execution state snapshot
   */
  async saveExecutionStateSnapshot(snapshot: AgentExecutionState): Promise<void> {
    if (!this.isInitialized) {
      logger.warn("Attempting to save execution state before initialization");
      return;
    }

    try {
      const stmt = this.db?.prepare(`
        INSERT INTO execution_states (execution_id, iteration, state_data, timestamp)
        VALUES (?, ?, ?, ?)
      `);

      stmt?.run(snapshot.executionId, snapshot.currentIteration, JSON.stringify(snapshot), Date.now());

      logger.debug("Saved execution state snapshot", {
        executionId: snapshot.executionId,
        iteration: snapshot.currentIteration,
      });
    } catch (error) {
      logger.error("Failed to save execution state snapshot", { error });
    }
  }

  /**
   * Get execution state snapshot
   */
  async getExecutionStateSnapshot(
    executionId: ID,
    iteration?: number,
  ): Promise<AgentExecutionState | null> {
    if (!this.isInitialized) {
      return null;
    }

    try {
      let stmt;
      let result;

      if (iteration !== undefined) {
        stmt = this.db?.prepare(`
          SELECT state_data FROM execution_states
          WHERE execution_id = ? AND iteration = ?
          LIMIT 1
        `);
        result = stmt?.get(executionId, iteration);
      } else {
        stmt = this.db?.prepare(`
          SELECT state_data FROM execution_states
          WHERE execution_id = ?
          ORDER BY iteration DESC
          LIMIT 1
        `);
        result = stmt?.get(executionId);
      }

      if (result && typeof result === 'object' && 'state_data' in result) {
        return JSON.parse((result as Record<string, unknown>)['state_data'] as string);
      }

      return null;
    } catch (error) {
      logger.error("Failed to get execution state snapshot", { error });
      return null;
    }
  }

  /**
   * List execution state snapshots
   */
  async listExecutionStateSnapshots(executionId: ID, limit?: number): Promise<AgentExecutionState[]> {
    if (!this.isInitialized) {
      return [];
    }

    try {
      const stmt = this.db?.prepare(`
        SELECT state_data FROM execution_states
        WHERE execution_id = ?
        ORDER BY iteration DESC
        LIMIT ?
      `);

      const results = stmt?.all(executionId, limit || 10) || [];

      return results
        .map((result: any) => {
          if (result !== null && typeof result === 'object' && 'state_data' in result) {
            return JSON.parse((result as Record<string, unknown>)['state_data'] as string);
          }
          return null;
        })
        .filter((s: any): s is AgentExecutionState => s !== null);
    } catch (error) {
      logger.error("Failed to list execution state snapshots", { error });
      return [];
    }
  }

  /**
   * Save resource usage record with iteration context
   */
  async saveResourceUsageRecord(
    executionId: ID,
    _record: ResourceUsageRecord,
    context?: { iteration?: number; cpuTime?: number; memoryUsed?: number },
  ): Promise<void> {
    if (!this.isInitialized) {
      return;
    }

    try {
      const stmt = this.db?.prepare(`
        INSERT INTO resource_usage (execution_id, iteration, cpu_time, memory_used, timestamp)
        VALUES (?, ?, ?, ?, ?)
      `);

      stmt?.run(
        executionId,
        context?.iteration ?? 0,
        context?.cpuTime ?? 0,
        context?.memoryUsed ?? 0,
        Date.now(),
      );
    } catch (error) {
      logger.error("Failed to save resource usage record", { error });
    }
  }

  /**
   * Get resource usage records
   */
  async getResourceUsageRecords(
    executionId: ID,
    filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
  ): Promise<ResourceUsageRecord[]> {
    if (!this.isInitialized) {
      return [];
    }

    try {
      let sql = `
        SELECT iteration, cpu_time, memory_used, timestamp
        FROM resource_usage
        WHERE execution_id = ?
      `;

      const params: unknown[] = [executionId];

      if (filter?.timeRange) {
        sql += ` AND timestamp >= ? AND timestamp <= ?`;
        params.push(filter.timeRange.start, filter.timeRange.end);
      }

      if (filter?.iterationRange) {
        sql += ` AND iteration >= ? AND iteration <= ?`;
        params.push(filter.iterationRange[0], filter.iterationRange[1]);
      }

      sql += ` ORDER BY iteration ASC`;

      const stmt = this.db?.prepare(sql);
      const results = stmt?.all(...params) || [];

      return results
        .map((r: any) => {
          if (typeof r === 'object' && r !== null && 'iteration' in r) {
            const record = r as Record<string, unknown>;
            // Map persisted resource usage to ResourceUsageRecord
            // Only include the fields that ResourceUsageRecord actually supports
            const resourceRecord: ResourceUsageRecord = {};

            // If we have memory peak data, include it
            if (record['memory_used']) {
              resourceRecord.memoryPeak = record['memory_used'] as number;
            }

            return resourceRecord;
          }
          return null;
        })
        .filter((r: any): r is ResourceUsageRecord => r !== null);
    } catch (error) {
      logger.error("Failed to get resource usage records", { error });
      return [];
    }
  }

  /**
   * Save system metrics record
   */
  async saveSystemMetrics(
    executionId: ID,
    iteration: number,
    metrics: IterationSystemMetrics,
  ): Promise<void> {
    if (!this.isInitialized) {
      return;
    }

    try {
      const stmt = this.db?.prepare(`
        INSERT INTO system_metrics (execution_id, iteration, cpu_time_ms, memory_peak_mb, duration_ms, timestamp)
        VALUES (?, ?, ?, ?, ?, ?)
      `);

      stmt?.run(
        executionId,
        iteration,
        metrics.cpuTimeMs,
        metrics.memoryPeakMb,
        metrics.durationMs,
        metrics.timestamp,
      );
    } catch (error) {
      logger.error("Failed to save system metrics", { error });
    }
  }

  /**
   * Save LLM metrics records
   */
  async saveLLMMetrics(
    executionId: ID,
    metrics: IterationLLMMetrics[],
  ): Promise<void> {
    if (!this.isInitialized) {
      return;
    }

    try {
      const stmt = this.db?.prepare(`
        INSERT INTO llm_metrics (execution_id, iteration, tool_call_id, input_tokens, output_tokens, cost_usd, model, duration_ms, timestamp)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
      `);

      for (const metric of metrics) {
        stmt?.run(
          executionId,
          metric.iteration,
          metric.toolCallId || null,
          metric.inputTokens,
          metric.outputTokens,
          metric.costUsd,
          metric.model,
          metric.durationMs,
          metric.timestamp,
        );
      }
    } catch (error) {
      logger.error("Failed to save LLM metrics", { error });
    }
  }

  /**
   * Get system metrics records
   */
  async getSystemMetrics(
    executionId: ID,
    filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
  ): Promise<IterationSystemMetrics[]> {
    if (!this.isInitialized) {
      return [];
    }

    try {
      let sql = `
        SELECT iteration, cpu_time_ms, memory_peak_mb, duration_ms, timestamp
        FROM system_metrics
        WHERE execution_id = ?
      `;

      const params: unknown[] = [executionId];

      if (filter?.timeRange) {
        sql += ` AND timestamp >= ? AND timestamp <= ?`;
        params.push(filter.timeRange.start, filter.timeRange.end);
      }

      if (filter?.iterationRange) {
        sql += ` AND iteration >= ? AND iteration <= ?`;
        params.push(filter.iterationRange[0], filter.iterationRange[1]);
      }

      sql += ` ORDER BY iteration ASC`;

      const stmt = this.db?.prepare(sql);
      const results = stmt?.all(...params) || [];

      return results
        .map((r: any) => {
          if (typeof r === 'object' && r !== null && 'iteration' in r) {
            const record = r as Record<string, unknown>;
            return {
              iteration: record['iteration'] as number,
              cpuTimeMs: record['cpu_time_ms'] as number,
              memoryPeakMb: record['memory_peak_mb'] as number,
              durationMs: record['duration_ms'] as number,
              timestamp: record['timestamp'] as number,
            };
          }
          return null;
        })
        .filter((m: any): m is IterationSystemMetrics => m !== null);
    } catch (error) {
      logger.error("Failed to get system metrics", { error });
      return [];
    }
  }

  /**
   * Get LLM metrics records
   */
  async getLLMMetrics(
    executionId: ID,
    filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
  ): Promise<IterationLLMMetrics[]> {
    if (!this.isInitialized) {
      return [];
    }

    try {
      let sql = `
        SELECT iteration, tool_call_id, input_tokens, output_tokens, cost_usd, model, duration_ms, timestamp
        FROM llm_metrics
        WHERE execution_id = ?
      `;

      const params: unknown[] = [executionId];

      if (filter?.timeRange) {
        sql += ` AND timestamp >= ? AND timestamp <= ?`;
        params.push(filter.timeRange.start, filter.timeRange.end);
      }

      if (filter?.iterationRange) {
        sql += ` AND iteration >= ? AND iteration <= ?`;
        params.push(filter.iterationRange[0], filter.iterationRange[1]);
      }

      sql += ` ORDER BY iteration ASC`;

      const stmt = this.db?.prepare(sql);
      const results = stmt?.all(...params) || [];

      return results
        .map((r: any) => {
          if (typeof r === 'object' && r !== null && 'iteration' in r) {
            const record = r as Record<string, unknown>;
            const metric: IterationLLMMetrics = {
              iteration: record['iteration'] as number,
              inputTokens: record['input_tokens'] as number,
              outputTokens: record['output_tokens'] as number,
              costUsd: record['cost_usd'] as number,
              model: record['model'] as string,
              durationMs: record['duration_ms'] as number,
              timestamp: record['timestamp'] as number,
            };
            if (record['tool_call_id']) {
              metric.toolCallId = record['tool_call_id'] as string;
            }
            return metric;
          }
          return null;
        })
        .filter((m: any): m is IterationLLMMetrics => m !== null);
    } catch (error) {
      logger.error("Failed to get LLM metrics", { error });
      return [];
    }
  }

  /**
   * Save event (queued for batch writing)
   */
  async saveEvent(executionId: ID, event: ExecutionEventRecord): Promise<void> {
    if (!this.isInitialized) {
      return;
    }

    this.eventQueue.push({
      executionId,
      event,
    });

    // Flush if batch size reached
    if (this.eventQueue.length >= this.batchSize) {
      await this.flushEvents();
    }
  }

  /**
   * Query events with filter
   */
  async queryEvents(filter: EventQueryFilter): Promise<ExecutionEventRecord[]> {
    if (!this.isInitialized) {
      return [];
    }

    try {
      let sql = 'SELECT event_data FROM events WHERE 1=1';
      const params: unknown[] = [];

      if (filter.executionId) {
        sql += ' AND execution_id = ?';
        params.push(filter.executionId);
      }

      if (filter.type && filter.type.length > 0) {
        sql += ` AND type IN (${filter.type.map(() => '?').join(',')})`;
        params.push(...filter.type);
      }

      if (filter.timeRange) {
        sql += ' AND timestamp >= ? AND timestamp <= ?';
        params.push(filter.timeRange.start, filter.timeRange.end);
      }

      if (filter.severity && filter.severity.length > 0) {
        sql += ` AND severity IN (${filter.severity.map(() => '?').join(',')})`;
        params.push(...filter.severity);
      }

      sql += ' ORDER BY timestamp DESC';

      if (filter.limit) {
        sql += ' LIMIT ?';
        params.push(filter.limit);
      }

      if (filter.offset) {
        sql += ' OFFSET ?';
        params.push(filter.offset);
      }

      const stmt = this.db?.prepare(sql);
      const results = stmt?.all(...params) || [];

      return results
        .map((r: any) => {
          if (r !== null && typeof r === 'object' && 'event_data' in r) {
            return JSON.parse((r as Record<string, unknown>)['event_data'] as string);
          }
          return null;
        })
        .filter((e: any): e is ExecutionEventRecord => e !== null);
    } catch (error) {
      logger.error("Failed to query events", { error });
      return [];
    }
  }

  /**
   * Count events matching filter
   */
  async countEvents(filter: EventQueryFilter): Promise<number> {
    if (!this.isInitialized) {
      return 0;
    }

    try {
      let sql = 'SELECT COUNT(*) as count FROM events WHERE 1=1';
      const params: unknown[] = [];

      if (filter.executionId) {
        sql += ' AND execution_id = ?';
        params.push(filter.executionId);
      }

      if (filter.type && filter.type.length > 0) {
        sql += ` AND type IN (${filter.type.map(() => '?').join(',')})`;
        params.push(...filter.type);
      }

      if (filter.timeRange) {
        sql += ' AND timestamp >= ? AND timestamp <= ?';
        params.push(filter.timeRange.start, filter.timeRange.end);
      }

      const stmt = this.db?.prepare(sql);
      const result = stmt?.get(...params);

      if (result !== null && typeof result === 'object' && 'count' in result) {
        return (result as Record<string, unknown>)['count'] as number;
      }

      return 0;
    } catch (error) {
      logger.error("Failed to count events", { error });
      return 0;
    }
  }

  /**
   * Save checkpoint metadata
   */
  async saveCheckpoint(checkpointId: string, metadata: Record<string, unknown>): Promise<void> {
    if (!this.isInitialized) {
      return;
    }

    try {
      const stmt = this.db?.prepare(`
        INSERT INTO checkpoints (checkpoint_id, metadata, timestamp)
        VALUES (?, ?, ?)
      `);

      stmt?.run(checkpointId, JSON.stringify(metadata), Date.now());
    } catch (error) {
      logger.error("Failed to save checkpoint", { error });
    }
  }

  /**
   * Get checkpoint history
   */
  async getCheckpointHistory(executionId: ID): Promise<Array<{ checkpointId: string; timestamp: number }>> {
    if (!this.isInitialized) {
      return [];
    }

    try {
      const stmt = this.db?.prepare(`
        SELECT checkpoint_id, timestamp FROM checkpoints
        WHERE execution_id = ?
        ORDER BY timestamp DESC
        LIMIT 50
      `);

      const results = stmt?.all(executionId) || [];

      return results
        .map((r: any) => {
          if (r !== null && typeof r === 'object' && 'checkpoint_id' in r && 'timestamp' in r) {
            return {
              checkpointId: (r as Record<string, unknown>)['checkpoint_id'] as string,
              timestamp: (r as Record<string, unknown>)['timestamp'] as number,
            };
          }
          return null;
        })
        .filter((c: any): c is { checkpointId: string; timestamp: number } => c !== null);
    } catch (error) {
      logger.error("Failed to get checkpoint history", { error });
      return [];
    }
  }

  /**
   * Private helper methods
   */

  private createSchema(): void {
    if (!this.db) {
      logger.warn("Database not initialized, skipping schema creation");
      return;
    }

    try {
      // Create tables
      this.db.exec(`
        CREATE TABLE IF NOT EXISTS events (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          execution_id TEXT NOT NULL,
          event_id TEXT NOT NULL,
          type TEXT NOT NULL,
          timestamp INTEGER NOT NULL,
          severity TEXT,
          event_data TEXT NOT NULL,
          created_at INTEGER DEFAULT (strftime('%s', 'now'))
        );

        CREATE TABLE IF NOT EXISTS execution_states (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          execution_id TEXT NOT NULL,
          iteration INTEGER,
          state_data TEXT NOT NULL,
          timestamp INTEGER NOT NULL,
          created_at INTEGER DEFAULT (strftime('%s', 'now'))
        );

        CREATE TABLE IF NOT EXISTS resource_usage (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          execution_id TEXT NOT NULL,
          iteration INTEGER,
          cpu_time REAL,
          memory_used INTEGER,
          timestamp INTEGER NOT NULL,
          created_at INTEGER DEFAULT (strftime('%s', 'now'))
        );

        CREATE TABLE IF NOT EXISTS system_metrics (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          execution_id TEXT NOT NULL,
          iteration INTEGER NOT NULL,
          cpu_time_ms REAL NOT NULL,
          memory_peak_mb REAL NOT NULL,
          duration_ms REAL NOT NULL,
          disk_io_bytes INTEGER,
          network_io_bytes INTEGER,
          timestamp INTEGER NOT NULL,
          created_at INTEGER DEFAULT (strftime('%s', 'now'))
        );

        CREATE TABLE IF NOT EXISTS llm_metrics (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          execution_id TEXT NOT NULL,
          iteration INTEGER NOT NULL,
          tool_call_id TEXT,
          input_tokens INTEGER NOT NULL,
          output_tokens INTEGER NOT NULL,
          cost_usd REAL NOT NULL,
          model TEXT NOT NULL,
          duration_ms REAL NOT NULL,
          cache_read_tokens INTEGER,
          cache_creation_tokens INTEGER,
          timestamp INTEGER NOT NULL,
          created_at INTEGER DEFAULT (strftime('%s', 'now'))
        );

        CREATE TABLE IF NOT EXISTS checkpoints (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          checkpoint_id TEXT NOT NULL,
          execution_id TEXT,
          metadata TEXT,
          timestamp INTEGER NOT NULL,
          created_at INTEGER DEFAULT (strftime('%s', 'now'))
        );

        -- Create indexes for common queries
        CREATE INDEX IF NOT EXISTS idx_events_execution_id ON events(execution_id);
        CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
        CREATE INDEX IF NOT EXISTS idx_execution_states_execution_id ON execution_states(execution_id);
        CREATE INDEX IF NOT EXISTS idx_resource_usage_execution_id ON resource_usage(execution_id);
        CREATE INDEX IF NOT EXISTS idx_system_metrics_execution_id ON system_metrics(execution_id);
        CREATE INDEX IF NOT EXISTS idx_llm_metrics_execution_id ON llm_metrics(execution_id);
      `);

      logger.debug("Database schema created successfully");
    } catch (error) {
      logger.error("Failed to create database schema", { error });
    }
  }

  private async flushEvents(): Promise<void> {
    if (this.eventQueue.length === 0 || !this.db) {
      return;
    }

    const toFlush = this.eventQueue.splice(0, this.batchSize);

    try {
      const stmt = this.db.prepare(`
        INSERT INTO events (execution_id, event_id, type, timestamp, severity, event_data)
        VALUES (?, ?, ?, ?, ?, ?)
      `);

      for (const { executionId, event } of toFlush) {
        stmt.run(
          executionId,
          event.id,
          event.type,
          event.timestamp,
          event.severity || 'info',
          JSON.stringify(event),
        );
      }

      this.lastFlushTime = Date.now();
      logger.debug("Flushed events to database", { count: toFlush.length });
    } catch (error) {
      logger.error("Failed to flush events", { error });
      // Re-queue failed events for retry on next flush interval
      this.eventQueue.unshift(...toFlush);
    }
  }

  private startFlushTimer(): void {
    if (this.flushTimer) {
      return;
    }

    this.flushTimer = setInterval(async () => {
      if (this.eventQueue.length > 0) {
        await this.flushEvents();
      }
    }, this.flushInterval);
  }

  private async cleanupOldRecords(): Promise<void> {
    if (!this.db) {
      return;
    }

    try {
      const cutoffTime = Date.now() - this.retentionDays * 24 * 60 * 60 * 1000;

      const stmt = this.db.prepare(`
        DELETE FROM events WHERE created_at < ?
      `);

      const result = stmt.run(cutoffTime);

      logger.info("Cleaned up old event records", { deleted: result.changes });
    } catch (error) {
      logger.error("Failed to cleanup old records", { error });
    }
  }

  private getLastFlushTime(): number | undefined {
    return this.lastFlushTime > 0 ? this.lastFlushTime : undefined;
  }
}
