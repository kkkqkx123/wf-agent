/**
 * ExecutionRecordManager - Shared Interruption & Event Record Management
 *
 * Consolidates the duplicated interruption and event record management logic
 * previously spread across WorkflowExecutionState and AgentLoopState.
 *
 * Design Principles:
 * - Single data source for interruption and event records
 * - Consistent trimming behavior (slice-based, keeps newest N records)
 * - Domain-neutral: the enriched interruption methods (recordPause/recordStop/recordResume)
 *   remain in domain-specific state classes since they carry domain-specific context
 * - Checkpoint-compatible via createSnapshot/restoreFromSnapshot
 *
 * Usage:
 *   const recordManager = new ExecutionRecordManager();
 *   recordManager.addInterruptionRecord(record);
 *   recordManager.addEventRecord(record);
 *   const stats = recordManager.getInterruptionStatistics();
 */

import type {
  ExecutionInterruptionRecord,
  ExecutionEventRecord,
} from "@wf-agent/types";
import {
  EXECUTION_STATE_MAX_INTERRUPTION_RECORDS,
  EXECUTION_STATE_MAX_EVENTS,
} from "@wf-agent/types";

/**
 * Snapshot type for ExecutionRecordManager
 */
export interface ExecutionRecordSnapshot {
  interruptionRecords: ExecutionInterruptionRecord[];
  eventRecords: ExecutionEventRecord[];
}

export class ExecutionRecordManager {
  /** Interruption records accumulated during execution */
  protected _interruptionRecords: ExecutionInterruptionRecord[] = [];

  /** Event records accumulated during execution */
  protected _eventRecords: ExecutionEventRecord[] = [];

  // ============================================================
  // Interruption Records
  // ============================================================

  /**
   * Add an interruption record
   * Keeps only the latest N records to prevent state bloat (slice-based).
   * @param record Interruption record to add
   */
  addInterruptionRecord(record: ExecutionInterruptionRecord): void {
    this._interruptionRecords.push(record);
    if (this._interruptionRecords.length > EXECUTION_STATE_MAX_INTERRUPTION_RECORDS) {
      this._interruptionRecords = this._interruptionRecords.slice(
        -EXECUTION_STATE_MAX_INTERRUPTION_RECORDS,
      );
    }
  }

  /**
   * Get all interruption records
   */
  getInterruptionRecords(): ExecutionInterruptionRecord[] {
    return [...this._interruptionRecords];
  }

  /**
   * Get interruption history with optional filtering
   * @param filter Optional filter: 'PAUSE' | 'STOP'
   * @returns Filtered interruption records
   */
  getInterruptionHistory(filter?: "PAUSE" | "STOP"): ExecutionInterruptionRecord[] {
    if (!filter) {
      return this.getInterruptionRecords();
    }
    return this._interruptionRecords.filter(record => record.type === filter);
  }

  /**
   * Get interruption statistics
   * @returns Statistics about interruptions: frequency, duration, recovery rate
   */
  getInterruptionStatistics(): {
    total: number;
    byType: Record<string, number>;
    averageDuration?: number;
    recoveryAttempts: number;
    successfulRecoveries: number;
    recoveryRate: number;
  } {
    if (this._interruptionRecords.length === 0) {
      return {
        total: 0,
        byType: {},
        recoveryAttempts: 0,
        successfulRecoveries: 0,
        recoveryRate: 0,
      };
    }

    const records = this._interruptionRecords;
    const byType: Record<string, number> = {};
    let totalDuration = 0;
    let durationCount = 0;
    let recoveryAttempts = 0;
    let successfulRecoveries = 0;

    records.forEach(record => {
      byType[record.type] = (byType[record.type] ?? 0) + 1;

      if (record.resumedAt && record.timestamp) {
        totalDuration += record.resumedAt - record.timestamp;
        durationCount++;
      }

      if (record.type === "PAUSE") {
        recoveryAttempts++;
      }

      if (record.status === "resumed") {
        successfulRecoveries++;
      }
    });

    return {
      total: records.length,
      byType,
      averageDuration: durationCount > 0 ? totalDuration / durationCount : undefined,
      recoveryAttempts,
      successfulRecoveries,
      recoveryRate: recoveryAttempts > 0
        ? (successfulRecoveries / recoveryAttempts) * 100
        : 0,
    };
  }

  /**
   * Find the latest PAUSE interruption record with 'pending' status
   * Used by recordResumeInterruption to enrich the pause record with resume info.
   */
  protected findLatestPendingPauseRecord(): ExecutionInterruptionRecord | undefined {
    for (let i = this._interruptionRecords.length - 1; i >= 0; i--) {
      const record = this._interruptionRecords[i];
      if (record?.type === "PAUSE" && record?.status === "pending") {
        return record;
      }
    }
    return undefined;
  }

  // ============================================================
  // Event Records
  // ============================================================

  /**
   * Add an event record
   * Keeps only the latest N records to prevent state bloat (slice-based).
   * @param record Event record to add
   */
  addEventRecord(record: ExecutionEventRecord): void {
    this._eventRecords.push(record);
    if (this._eventRecords.length > EXECUTION_STATE_MAX_EVENTS) {
      this._eventRecords = this._eventRecords.slice(-EXECUTION_STATE_MAX_EVENTS);
    }
  }

  /**
   * Get all event records
   */
  getEventRecords(): ExecutionEventRecord[] {
    return [...this._eventRecords];
  }

  // ============================================================
  // Lifecycle & State Management
  // ============================================================

  /**
   * Clean up resources
   */
  cleanup(): void {
    this._interruptionRecords = [];
    this._eventRecords = [];
  }

  /**
   * Get the total number of records managed
   */
  size(): number {
    return this._interruptionRecords.length + this._eventRecords.length;
  }

  /**
   * Check if no records are managed
   */
  isEmpty(): boolean {
    return this._interruptionRecords.length === 0 && this._eventRecords.length === 0;
  }

  /**
   * Reset to initial state
   */
  reset(): void {
    this.cleanup();
  }

  /**
   * Create a snapshot of the current records
   */
  createSnapshot(): ExecutionRecordSnapshot {
    return {
      interruptionRecords: this._interruptionRecords.map(r => ({ ...r })),
      eventRecords: this._eventRecords.map(r => ({ ...r })),
    };
  }

  /**
   * Restore records from snapshot
   * @param snapshot The state snapshot
   */
  restoreFromSnapshot(snapshot: ExecutionRecordSnapshot): void {
    this._interruptionRecords = snapshot.interruptionRecords.map(r => ({ ...r }));
    this._eventRecords = snapshot.eventRecords.map(r => ({ ...r }));
  }
}