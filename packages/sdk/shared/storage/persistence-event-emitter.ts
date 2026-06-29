/**
 * Persistence Event Emitter
 *
 * Provides event notification for storage persistence operations.
 * Enables applications to respond to persistence failures in ASYNC mode.
 *
 * Events:
 * - persist_started: Operation started
 * - persist_success: Operation completed successfully
 * - persist_failed: Operation failed (ASYNC mode only)
 *
 * Failed operations are tracked for diagnostics and automatically cleaned up
 * after 1 hour to prevent memory leaks in long-running systems.
 */

import { EventEmitter } from "events";

export interface PersistenceEvent {
  entityId: string;
  operation: "save" | "delete" | "update";
  timestamp: number;
}

export interface PersistenceFailedEvent extends PersistenceEvent {
  error: Error;
}

export interface PersistenceSuccessEvent extends PersistenceEvent {
  dataSize?: number;
}

/**
 * Event-based notification system for persistence operations
 * Allows applications to monitor storage operations in ASYNC mode
 */
export class PersistenceEventEmitter extends EventEmitter {
  /** Maximum age for failed operations before auto-cleanup (1 hour) */
  private readonly FAILED_OPS_MAX_AGE_MS = 3600 * 1000;

  private failedOperations: Map<string, PersistenceFailedEvent & { timestamp: number }> = new Map();

  /**
   * Emit persist started event
   */
  notifyPersistStarted(entityId: string, operation: "save" | "delete" | "update"): void {
    const event: PersistenceEvent = {
      entityId,
      operation,
      timestamp: Date.now(),
    };
    this.emit("persist_started", event);
  }

  /**
   * Emit persist success event
   */
  notifyPersistSuccess(
    entityId: string,
    operation: "save" | "delete" | "update",
    dataSize?: number,
  ): void {
    const event: PersistenceSuccessEvent = {
      entityId,
      operation,
      timestamp: Date.now(),
      dataSize,
    };
    this.emit("persist_success", event);

    // Clear failed record if exists
    this.failedOperations.delete(entityId);
  }

  /**
   * Emit persist failed event
   * @param entityId Entity ID
   * @param operation Operation type
   * @param error Error that occurred
   */
  notifyPersistFailed(
    entityId: string,
    operation: "save" | "delete" | "update",
    error: Error,
  ): void {
    const event: PersistenceFailedEvent & { timestamp: number } = {
      entityId,
      operation,
      timestamp: Date.now(),
      error,
    };
    this.emit("persist_failed", event);

    // Track failed operation for diagnostics
    this.failedOperations.set(entityId, event);

    // Cleanup old failures to prevent memory leaks
    this.cleanupOldFailures();
  }

  /**
   * Listen for persist started events
   */
  onPersistStarted(callback: (event: PersistenceEvent) => void): this {
    return this.on("persist_started", callback);
  }

  /**
   * Listen for persist success events
   */
  onPersistSuccess(callback: (event: PersistenceSuccessEvent) => void): this {
    return this.on("persist_success", callback);
  }

  /**
   * Listen for persist failed events
   */
  onPersistFailed(callback: (event: PersistenceFailedEvent) => void): this {
    return this.on("persist_failed", callback);
  }

  /**
   * Get all failed operations
   */
  getFailedOperations(): PersistenceFailedEvent[] {
    return Array.from(this.failedOperations.values());
  }

  /**
   * Get failed operation count
   */
  getFailedCount(): number {
    return this.failedOperations.size;
  }

  /**
   * Clear all failed operations from tracking
   * Useful for manual cleanup or testing
   */
  clearFailedOperations(): void {
    this.failedOperations.clear();
  }

  /**
   * Remove expired failed operations to prevent memory leaks
   * Automatically called after each persist failure
   * @private
   */
  private cleanupOldFailures(): void {
    const now = Date.now();
    for (const [id, event] of this.failedOperations.entries()) {
      if (now - event.timestamp > this.FAILED_OPS_MAX_AGE_MS) {
        this.failedOperations.delete(id);
      }
    }
  }
}
