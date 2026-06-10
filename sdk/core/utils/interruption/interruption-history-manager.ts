/**
 * Interruption History Manager
 * 
 * Records interruption events for debugging, analysis, and auditing purposes.
 * 
 * Features:
 * - Records all interruption events (PAUSE/STOP/RESUME)
 * - Configurable history size limit
 * - Filtering and querying capabilities
 * - Export functionality
 */

import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "InterruptionHistoryManager" });

/**
 * Interruption history entry
 */
export interface InterruptionHistoryEntry {
  /** Unique identifier */
  id: string;
  /** Timestamp of the event */
  timestamp: number;
  /** Type of interruption */
  type: "PAUSE" | "STOP" | "RESUME";
  /** Context ID (execution ID, session ID, etc.) */
  contextId: string;
  /** Node ID (optional) */
  nodeId?: string;
  /** Iteration number (optional, for Agent loops) */
  iteration?: number;
  /** Who triggered the interruption */
  triggeredBy?: "user" | "system" | "timeout" | "error";
  /** Additional metadata */
  metadata?: Record<string, unknown>;
  /** Duration in milliseconds (for RESUME events, indicates pause duration) */
  duration?: number;
}

/**
 * Filter options for querying history
 */
export interface HistoryFilter {
  /** Filter by context ID */
  contextId?: string;
  /** Filter by interruption type */
  type?: "PAUSE" | "STOP" | "RESUME";
  /** Filter by trigger source */
  triggeredBy?: "user" | "system" | "timeout" | "error";
  /** Get entries since this timestamp */
  since?: number;
  /** Get entries before this timestamp */
  before?: number;
  /** Maximum number of entries to return */
  limit?: number;
}

/**
 * Interruption History Manager
 * 
 * Maintains a bounded history of interruption events for debugging and analysis.
 */
export class InterruptionHistoryManager {
  private history: InterruptionHistoryEntry[] = [];
  private maxSize: number;
  private entryCounter: number = 0;

  constructor(maxSize: number = 1000) {
    this.maxSize = maxSize;
    logger.info("Interruption history manager initialized", { maxSize });
  }

  /**
   * Record an interruption event
   * 
   * @param entry Event data (without id and timestamp)
   * @returns The complete entry with generated id and timestamp
   */
  record(entry: Omit<InterruptionHistoryEntry, "id" | "timestamp">): InterruptionHistoryEntry {
    const fullEntry: InterruptionHistoryEntry = {
      ...entry,
      id: this.generateId(),
      timestamp: Date.now(),
    };

    this.history.push(fullEntry);

    // Maintain history size limit
    if (this.history.length > this.maxSize) {
      const removed = this.history.length - this.maxSize;
      this.history = this.history.slice(-this.maxSize);
      
      logger.debug("History size limit reached, removed old entries", {
        removed,
        newSize: this.history.length,
      });
    }

    logger.debug("Interruption event recorded", {
      type: fullEntry.type,
      contextId: fullEntry.contextId,
      triggeredBy: fullEntry.triggeredBy,
    });

    return fullEntry;
  }

  /**
   * Get interruption history with optional filtering
   * 
   * @param filter Filter options
   * @returns Filtered history entries (newest first)
   */
  getHistory(filter?: HistoryFilter): InterruptionHistoryEntry[] {
    let result = [...this.history];

    // Apply filters
    if (filter?.contextId) {
      result = result.filter(e => e.contextId === filter.contextId);
    }

    if (filter?.type) {
      result = result.filter(e => e.type === filter.type);
    }

    if (filter?.triggeredBy) {
      result = result.filter(e => e.triggeredBy === filter.triggeredBy);
    }

    if (filter?.since) {
      result = result.filter(e => e.timestamp >= filter.since!);
    }

    if (filter?.before) {
      result = result.filter(e => e.timestamp <= filter.before!);
    }

    // Sort by timestamp descending (newest first).
    // Use insertion-order index as tiebreaker so entries recorded within the same
    // millisecond are deterministically ordered (later insertion = newer).
    result.sort((a, b) => {
      const tsDiff = b.timestamp - a.timestamp;
      if (tsDiff !== 0) return tsDiff;
      return b.id.localeCompare(a.id);
    });

    // Apply limit
    if (filter?.limit && filter.limit > 0) {
      result = result.slice(0, filter.limit);
    }

    return result;
  }

  /**
   * Get the most recent interruption event for a context
   * 
   * @param contextId Context ID
   * @returns Most recent entry or undefined
   */
  getLatestEvent(contextId: string): InterruptionHistoryEntry | undefined {
    const entries = this.getHistory({ contextId, limit: 1 });
    return entries.length > 0 ? entries[0] : undefined;
  }

  /**
   * Get pause duration for a context
   *
   * Calculates the time between the most recent PAUSE and its matching RESUME.
   * Iterates chronologically (oldest-first) to correctly pair PAUSE → RESUME
   * in their causal order, avoiding mismatches from newest-first traversal.
   *
   * @param contextId Context ID
   * @returns Pause duration in milliseconds, or null if not available
   */
  getPauseDuration(contextId: string): number | null {
    const entries = this.getHistory({ contextId });

    // Iterate chronologically (oldest first = reverse of newest-first order)
    // so we correctly pair each RESUME with its preceding PAUSE.
    let lastPauseTimestamp: number | null = null;

    for (let i = entries.length - 1; i >= 0; i--) {
      const entry = entries[i];
      if (!entry) continue;
      if (entry.type === "PAUSE") {
        lastPauseTimestamp = entry.timestamp;
      } else if (entry.type === "RESUME" && lastPauseTimestamp !== null) {
        return entry.timestamp - lastPauseTimestamp;
      }
    }

    return null;
  }

  /**
   * Get statistics about interruptions
   * 
   * @param contextId Optional context ID to filter by
   * @returns Statistics object
   */
  getStatistics(contextId?: string): {
    totalEvents: number;
    pauseCount: number;
    stopCount: number;
    resumeCount: number;
    avgPauseDuration?: number;
    userTriggeredCount: number;
    systemTriggeredCount: number;
    timeoutTriggeredCount: number;
  } {
    const entries = contextId ? this.getHistory({ contextId }) : this.history;

    const pauseCount = entries.filter(e => e.type === "PAUSE").length;
    const stopCount = entries.filter(e => e.type === "STOP").length;
    const resumeCount = entries.filter(e => e.type === "RESUME").length;

    // Calculate average pause duration
    let avgPauseDuration: number | undefined;
    const pauseDurations: number[] = [];
    
    for (const entry of entries) {
      if (entry.type === "RESUME" && entry.duration) {
        pauseDurations.push(entry.duration);
      }
    }
    
    if (pauseDurations.length > 0) {
      avgPauseDuration = pauseDurations.reduce((sum, d) => sum + d, 0) / pauseDurations.length;
    }

    return {
      totalEvents: entries.length,
      pauseCount,
      stopCount,
      resumeCount,
      avgPauseDuration,
      userTriggeredCount: entries.filter(e => e.triggeredBy === "user").length,
      systemTriggeredCount: entries.filter(e => e.triggeredBy === "system").length,
      timeoutTriggeredCount: entries.filter(e => e.triggeredBy === "timeout").length,
    };
  }

  /**
   * Export all history entries
   * 
   * @returns Copy of all history entries
   */
  export(): InterruptionHistoryEntry[] {
    return [...this.history];
  }

  /**
   * Clear all history
   */
  clear(): void {
    const count = this.history.length;
    this.history = [];
    logger.info("History cleared", { clearedCount: count });
  }

  /**
   * Get current history size
   */
  getSize(): number {
    return this.history.length;
  }

  /**
   * Update maximum history size
   * 
   * @param newSize New maximum size
   */
  setMaxSize(newSize: number): void {
    this.maxSize = newSize;
    
    // Trim if necessary
    if (this.history.length > newSize) {
      this.history = this.history.slice(-newSize);
      logger.info("History trimmed to new size limit", { newSize });
    }
  }

  /**
   * Generate unique ID for history entries
   */
  private generateId(): string {
    this.entryCounter++;
    return `int_hist_${Date.now()}_${this.entryCounter}`;
  }
}
