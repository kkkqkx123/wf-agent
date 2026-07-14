/**
 * Timeout Manager State Manager
 *
 * Manages individual timeout lifecycle within an execution context.
 * Provides registration, cancellation, refresh, and state management capabilities.
 */

import { StateManager } from "../types/state-manager.js";
import type {
  TimeoutRegistration,
  TimeoutHandle,
  TimeoutEntry,
  TimeoutSnapshot,
  TimeoutStats,
  InterruptionStateReference,
  TimeoutManagerConfig,
  ResolvedTimeoutManagerConfig,
} from "../types/timeout.js";
import type { BaseEvent, EventType } from "@wf-agent/types";
import { DEFAULT_TIMEOUT_MANAGER_CONFIG } from "../types/timeout.js";
import { isValidTimeoutTag } from "../types/timeout-tags.js";
import type { EventRegistry } from "../registry/event-registry.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TimeoutManager" });

/**
 * Timeout entry with handle methods
 */
interface TimeoutEntryWithHandle extends TimeoutEntry {
  handle: TimeoutHandle | null;
  /** Timestamp when this entry was paused (null if not paused) */
  pausedAt?: number;
  /** Total accumulated paused duration in milliseconds */
  totalPausedDuration?: number;
}

/**
 * TimeoutManager configuration with resolved defaults
 */
interface TimeoutManagerInternalConfig extends ResolvedTimeoutManagerConfig {
  /** Maximum number of timeouts per execution */
  maxTimeoutsPerExecution: number;
}

/**
 * Timeout Manager
 *
 * Manages the lifecycle of timeouts within a single execution context.
 * Each execution should have its own TimeoutManager instance.
 *
 * Features:
 * - Register timeouts with automatic cleanup
 * - Integration with InterruptionState for automatic cancellation
 * - Warning support with configurable thresholds
 * - Checkpoint serialization for state persistence
 * - Metrics collection for observability
 * - Event emission for lifecycle tracking
 */
export class TimeoutManager implements StateManager<TimeoutSnapshot> {
  /** Map of active timeouts by ID */
  private timeouts: Map<string, TimeoutEntryWithHandle> = new Map();

  /** Resolved configuration */
  private config: TimeoutManagerInternalConfig;

  /** Event registry for emitting timeout events */
  private eventRegistry?: EventRegistry;

  /** Execution ID for event scoping */
  private executionId?: string;

  /** Statistics tracking */
  private stats: {
    totalRegistered: number;
    timedOutCount: number;
    cancelledCount: number;
    durations: number[];
    byTag: Map<string, number>;
    byModule: Map<string, number>;
  } = {
    totalRegistered: 0,
    timedOutCount: 0,
    cancelledCount: 0,
    durations: [],
    byTag: new Map(),
    byModule: new Map(),
  };

  /**
   * Create a new TimeoutManager
   * @param config Optional configuration (will be merged with defaults)
   * @param options Optional event registry and execution ID for event emission
   */
  constructor(
    config?: TimeoutManagerConfig,
    options?: {
      eventRegistry?: EventRegistry;
      executionId?: string;
    },
  ) {
    this.config = {
      ...DEFAULT_TIMEOUT_MANAGER_CONFIG,
      ...config,
    };
    this.eventRegistry = options?.eventRegistry;
    this.executionId = options?.executionId;
  }

  /**
   * Register a new timeout
   * @param options Timeout registration options
   * @returns TimeoutHandle for managing the timeout
   */
  register(options: TimeoutRegistration): TimeoutHandle {
    const {
      id,
      duration,
      onTimeout,
      warningThreshold,
      onWarning,
      interruptionState,
      tag,
      metadata,
    } = options;

    // Validate timeout ID
    if (!id || id.trim().length === 0) {
      throw new Error("Timeout ID cannot be empty");
    }

    // Validate duration
    const validatedDuration = this.validateDuration(duration);

    // Validate tag if provided
    if (tag && !isValidTimeoutTag(tag)) {
      logger.warn(
        `Timeout '${id}' uses non-standard tag '${tag}'. Consider using standard tags from timeout-tags.ts`,
      );
    }

    // Check if timeout already exists
    if (this.timeouts.has(id)) {
      throw new Error(`Timeout with ID '${id}' already exists`);
    }

    // Check max timeouts limit
    if (this.timeouts.size >= this.config.maxTimeoutsPerExecution) {
      throw new Error(
        `Maximum number of timeouts (${this.config.maxTimeoutsPerExecution}) reached`,
      );
    }

    // Calculate warning time
    const effectiveWarningThreshold =
      warningThreshold ??
      (this.config.enableWarnings ? this.config.defaultWarningThreshold : undefined);

    // Validate warning threshold
    if (effectiveWarningThreshold !== undefined) {
      if (effectiveWarningThreshold <= 0) {
        throw new Error("Warning threshold must be positive");
      }
      if (effectiveWarningThreshold >= validatedDuration) {
        logger.warn(
          `Timeout '${id}': warning threshold (${effectiveWarningThreshold}ms) is greater than or equal to duration (${validatedDuration}ms)`,
        );
      }
    }

    const startTime = Date.now();

    // Ensure tag is included in metadata for diagnostic purposes
    const enrichedMetadata = metadata ? { ...metadata } : {};
    if (tag) {
      enrichedMetadata["tag"] = tag;
    }

    // Create timeout entry
    const entry: TimeoutEntryWithHandle = {
      id,
      startTime,
      duration: validatedDuration,
      status: "active",
      warningEmitted: false,
      onTimeout,
      onWarning,
      metadata: enrichedMetadata,
      handle: null, // Will be set below
    };

    // Set up main timeout timer
    const timerId = setTimeout(async () => {
      await this.handleTimeoutExpiration(entry);
    }, validatedDuration);
    entry.timerId = timerId;

    // Set up warning timer if configured
    if (effectiveWarningThreshold && onWarning && this.config.enableWarnings) {
      const warningDelay = validatedDuration - effectiveWarningThreshold;
      if (warningDelay > 0) {
        const warningTimerId = setTimeout(async () => {
          await this.handleWarning(entry, effectiveWarningThreshold);
        }, warningDelay);
        entry.warningTimerId = warningTimerId;
      }
    }

    // Bind to interruption state if provided
    if (interruptionState) {
      this.bindToInterruptionState(entry, interruptionState);
    }

    // Create handle
    const handle = this.createHandle(entry);
    entry.handle = handle;

    // Store entry with handle
    this.timeouts.set(id, entry);

    // Update statistics
    this.stats.totalRegistered++;
    if (tag) {
      this.stats.byTag.set(tag, (this.stats.byTag.get(tag) || 0) + 1);
    }

    // Emit registration event
    this.emitEvent("TIMEOUT_REGISTERED", {
      timeoutId: id,
      duration: validatedDuration,
      tag,
      metadata,
    });

    return handle;
  }

  /**
   * Register a timeout with automatic interruption protection
   *
   * This is a convenience method that automatically binds the timeout to the
   * current interruption state, ensuring it's cancelled when execution is interrupted.
   *
   * @param options Timeout registration options (interruptionState will be set automatically)
   * @param interruptionState The interruption state to bind to
   * @returns TimeoutHandle for managing the timeout
   *
   * @example
   * ```typescript
   * const handle = manager.registerWithInterruptionProtection(
   *   {
   *     id: 'llm-call',
   *     duration: 30000,
   *     onTimeout: () => cancelLLMCall(),
   *     tag: 'llm-call'
   *   },
   *   interruptionState
   * );
   * ```
   */
  registerWithInterruptionProtection(
    options: Omit<TimeoutRegistration, "interruptionState">,
    interruptionState: InterruptionStateReference,
  ): TimeoutHandle {
    return this.register({
      ...options,
      interruptionState,
    });
  }

  /**
   * Cancel a timeout by handle
   * @param handle TimeoutHandle to cancel
   */
  cancel(handle: TimeoutHandle): void {
    const entry = this.timeouts.get(handle.id);
    if (!entry) {
      return; // Already cancelled or doesn't exist
    }

    try {
      this.cancelEntry(entry, "cancelled");
    } catch (error) {
      logger.error(`Failed to cancel timeout ${handle.id}`, { error });
    }
  }

  /**
   * Refresh a timeout (reset the timer)
   * @param handle TimeoutHandle to refresh
   */
  refresh(handle: TimeoutHandle): void {
    const entry = this.timeouts.get(handle.id);
    if (!entry || entry.status !== "active") {
      return; // Can't refresh inactive timeout
    }

    try {
      // Clear existing timers
      if (entry.timerId) {
        clearTimeout(entry.timerId);
      }
      if (entry.warningTimerId) {
        clearTimeout(entry.warningTimerId);
      }

      // Reset start time
      entry.startTime = Date.now();
      entry.warningEmitted = false;

      // Re-set main timeout
      const timerId = setTimeout(async () => {
        await this.handleTimeoutExpiration(entry);
      }, entry.duration);
      entry.timerId = timerId;

      // Re-set warning timer if applicable
      if (entry.onWarning && this.config.enableWarnings) {
        const warningDelay = entry.duration - this.config.defaultWarningThreshold;
        if (warningDelay > 0) {
          const warningTimerId = setTimeout(async () => {
            await this.handleWarning(entry, this.config.defaultWarningThreshold);
          }, warningDelay);
          entry.warningTimerId = warningTimerId;
        }
      }
    } catch (error) {
      logger.error(`Failed to refresh timeout ${handle.id}`, { error });
    }
  }

  /**
   * Get remaining time for a timeout
   * @param handle TimeoutHandle
   * @returns Remaining time in milliseconds, or 0 if expired/cancelled
   */
  getRemainingTime(handle: TimeoutHandle): number {
    const entry = this.timeouts.get(handle.id);
    if (!entry || entry.status !== "active") {
      return 0;
    }

    const elapsed = Date.now() - entry.startTime;
    const remaining = entry.duration - elapsed;
    return Math.max(0, remaining);
  }

  /**
   * Get statistics about timeout usage
   * @returns TimeoutStats
   */
  getStats(): TimeoutStats {
    const avgDuration =
      this.stats.durations.length > 0
        ? this.stats.durations.reduce((a, b) => a + b, 0) / this.stats.durations.length
        : 0;

    const byTag: Record<string, number> = {};
    this.stats.byTag.forEach((count, tag) => {
      byTag[tag] = count;
    });

    const byModule: Record<string, number> = {};
    this.stats.byModule.forEach((count, module) => {
      byModule[module] = count;
    });

    return {
      activeTimeouts: this.size(),
      totalRegistered: this.stats.totalRegistered,
      timedOutCount: this.stats.timedOutCount,
      cancelledCount: this.stats.cancelledCount,
      averageDuration: avgDuration,
      byTag,
      byModule,
    };
  }

  /**
   * Pause all active timeouts
   *
   * Clears all active timers and records the pause timestamp.
   * Use resume() to restore timeouts with adjusted remaining time.
   * Safe to call multiple times (idempotent).
   */
  pause(): void {
    const now = Date.now();
    this.timeouts.forEach(entry => {
      if (entry.status === "active" && !entry.pausedAt) {
        // Record pause start time
        entry.pausedAt = now;
        // Clear the main timer
        if (entry.timerId) {
          clearTimeout(entry.timerId);
          entry.timerId = undefined;
        }
        // Clear the warning timer
        if (entry.warningTimerId) {
          clearTimeout(entry.warningTimerId);
          entry.warningTimerId = undefined;
        }
      }
    });
    logger.debug("Paused all active timeouts", { count: this.size() });
  }

  /**
   * Resume all paused timeouts
   *
   * Adjusts startTime by the pause duration and re-creates timers
   * with the remaining time. Safe to call multiple times (idempotent).
   */
  resume(): void {
    const now = Date.now();
    this.timeouts.forEach(entry => {
      if (entry.status === "active" && entry.pausedAt) {
        const pauseDuration = now - entry.pausedAt;
        entry.totalPausedDuration = (entry.totalPausedDuration || 0) + pauseDuration;
        entry.pausedAt = undefined;

        // Calculate remaining time
        const elapsed = now - entry.startTime - (entry.totalPausedDuration || 0);
        const remaining = Math.max(0, entry.duration - elapsed);

        // Re-create main timer with remaining time
        if (remaining > 0) {
          const timerId = setTimeout(async () => {
            await this.handleTimeoutExpiration(entry);
          }, remaining);
          entry.timerId = timerId;

          // Re-create warning timer if applicable
          if (entry.onWarning && this.config.enableWarnings) {
            const warningRemaining = Math.max(0, entry.duration - this.config.defaultWarningThreshold - elapsed);
            if (warningRemaining > 0 && !entry.warningEmitted) {
              const warningTimerId = setTimeout(async () => {
                await this.handleWarning(entry, this.config.defaultWarningThreshold);
              }, warningRemaining);
              entry.warningTimerId = warningTimerId;
            }
          }
        } else {
          // Timeout already expired during pause, fire immediately
          this.handleTimeoutExpiration(entry);
        }
      }
    });
    logger.debug("Resumed all paused timeouts");
  }

  /**
   * Get effective elapsed time for a timeout handle
   * @param handle TimeoutHandle
   * @returns Effective elapsed time (excluding paused duration), or 0 if not found
   */
  getEffectiveElapsed(handle: TimeoutHandle): number {
    const entry = this.timeouts.get(handle.id);
    if (!entry) return 0;
    const elapsed = Date.now() - entry.startTime - (entry.totalPausedDuration || 0);
    return Math.max(0, elapsed);
  }

  // ==================== StateManager Interface ====================

  /**
   * Get number of active timeouts
   */
  size(): number {
    let count = 0;
    this.timeouts.forEach(entry => {
      if (entry.status === "active") {
        count++;
      }
    });
    return count;
  }

  /**
   * Check if there are no active timeouts
   */
  isEmpty(): boolean {
    return this.size() === 0;
  }

  /**
   * Clear all timeouts
   */
  clear(): void {
    this.timeouts.forEach(entry => {
      this.cancelEntry(entry, "cancelled");
    });
    this.timeouts.clear();
  }

  /**
   * Cleanup all resources (StateManager interface)
   */
  cleanup(): void {
    this.clear();
  }

  /**
   * Create a state snapshot (StateManager interface)
   * @returns TimeoutSnapshot
   */
  createSnapshot(): TimeoutSnapshot {
    return this.serialize();
  }

  /**
   * Restore from a snapshot (StateManager interface)
   * Note: Timers are not restored, only metadata
   * @param snapshot TimeoutSnapshot to restore
   */
  restoreFromSnapshot(snapshot: TimeoutSnapshot): void {
    this.restore(snapshot);
  }

  /**
   * Serialize timeout state for checkpoint
   * @returns TimeoutSnapshot
   */
  serialize(): TimeoutSnapshot {
    const timeouts: TimeoutSnapshot["timeouts"] = [];

    this.timeouts.forEach(entry => {
      timeouts.push({
        id: entry.id,
        startTime: entry.startTime,
        duration: entry.duration,
        status: entry.status,
        warningEmitted: entry.warningEmitted,
        totalPausedDuration: entry.totalPausedDuration,
        metadata: entry.metadata,
      });
    });

    return {
      version: 1,
      timestamp: Date.now(),
      timeouts,
    };
  }

  /**
   * Restore timeout state from checkpoint
   * Note: Timers are not restored, only metadata
   * @param snapshot TimeoutSnapshot to restore
   */
  restore(snapshot: TimeoutSnapshot): void {
    // Clear existing timeouts
    this.clear();

    // Restore metadata (timers are not restored)
    snapshot.timeouts.forEach(timeoutData => {
      // We don't re-create timers on restore
      // The calling code should re-register timeouts as needed
      // This is mainly for tracking and metrics purposes
      const entry: TimeoutEntryWithHandle = {
        id: timeoutData.id,
        startTime: timeoutData.startTime,
        duration: timeoutData.duration,
        status: timeoutData.status,
        warningEmitted: timeoutData.warningEmitted,
        onTimeout: () => {}, // Placeholder, will be re-registered
        totalPausedDuration: timeoutData.totalPausedDuration,
        metadata: timeoutData.metadata,
        handle: null, // Will be set below
      };

      const handle = this.createHandle(entry);
      entry.handle = handle;
      this.timeouts.set(timeoutData.id, entry);
    });
  }

  // ==================== Private Methods ====================

  /**
   * Validate and normalize timeout duration
   */
  private validateDuration(duration: number): number {
    if (duration <= 0) {
      throw new Error("Timeout duration must be positive");
    }

    if (duration > this.config.maxTimeout) {
      logger.warn(
        `Timeout duration ${duration}ms exceeds maximum ${this.config.maxTimeout}ms, capping to maximum`,
      );
      return this.config.maxTimeout;
    }

    return duration;
  }

  /**
   * Create a TimeoutHandle for an entry
   */
  private createHandle(entry: TimeoutEntryWithHandle): TimeoutHandle {
    return {
      id: entry.id,
      isActive: () => entry.status === "active",
      getRemainingTime: () => this.getRemainingTimeFromEntry(entry),
      cancel: () => this.cancelEntry(entry, "cancelled"),
    };
  }

  /**
   * Get remaining time from entry directly
   */
  private getRemainingTimeFromEntry(entry: TimeoutEntry): number {
    if (entry.status !== "active") {
      return 0;
    }

    const elapsed = Date.now() - entry.startTime;
    const remaining = entry.duration - elapsed;
    return Math.max(0, remaining);
  }

  /**
   * Cancel a timeout entry
   */
  private cancelEntry(entry: TimeoutEntryWithHandle, _reason: string): void {
    if (entry.status !== "active") {
      return; // Already cancelled or expired
    }

    // Clear timers
    if (entry.timerId) {
      clearTimeout(entry.timerId);
    }
    if (entry.warningTimerId) {
      clearTimeout(entry.warningTimerId);
    }

    // Unsubscribe from interruption state
    if (entry.interruptionUnsubscribe) {
      entry.interruptionUnsubscribe();
    }

    // Update status
    entry.status = "cancelled";

    // Update statistics
    this.stats.cancelledCount++;
    this.stats.durations.push(Date.now() - entry.startTime);

    // Emit cancellation event
    this.emitEvent("TIMEOUT_CANCELLED", {
      timeoutId: entry.id,
      reason: _reason,
      tag: entry.metadata?.["tag"] as string | undefined,
    });
  }

  /**
   * Handle timeout expiration
   */
  private async handleTimeoutExpiration(entry: TimeoutEntryWithHandle): Promise<void> {
    if (entry.status !== "active") {
      return;
    }

    // Update status
    entry.status = "expired";

    // Calculate actual duration
    const actualDuration = Date.now() - entry.startTime;

    // Update statistics
    this.stats.timedOutCount++;
    this.stats.durations.push(actualDuration);

    // Log timeout expiration
    const tagInfo = entry.metadata?.["tag"] ? ` (tag: ${entry.metadata["tag"]})` : "";
    logger.debug(`Timeout '${entry.id}' expired after ${actualDuration}ms${tagInfo}`);

    // Emit expiration event
    this.emitEvent("TIMEOUT_EXPIRED", {
      timeoutId: entry.id,
      actualDuration,
      configuredDuration: entry.duration,
      tag: entry.metadata?.["tag"] as string | undefined,
    });

    // Execute timeout callback with error handling
    try {
      await entry.onTimeout();
    } catch (error) {
      logger.error(`Error in timeout callback for '${entry.id}'`, { error });
    }
  }

  /**
   * Handle warning threshold reached
   */
  private async handleWarning(
    entry: TimeoutEntryWithHandle,
    warningThreshold: number,
  ): Promise<void> {
    if (entry.status !== "active" || entry.warningEmitted) {
      return;
    }

    entry.warningEmitted = true;

    // Calculate remaining time
    const remainingTime = this.getRemainingTimeFromEntry(entry);

    // Log warning
    const tagInfo = entry.metadata?.["tag"] ? ` (tag: ${entry.metadata["tag"]})` : "";
    logger.debug(
      `Timeout '${entry.id}' warning: ${remainingTime}ms remaining (threshold: ${warningThreshold}ms)${tagInfo}`,
    );

    // Emit warning event
    this.emitEvent("TIMEOUT_WARNING", {
      timeoutId: entry.id,
      remainingTime,
      warningThreshold,
      tag: entry.metadata?.["tag"] as string | undefined,
    });

    // Execute warning callback with error handling
    if (entry.onWarning) {
      try {
        await entry.onWarning();
      } catch (error: unknown) {
        logger.error(`Error in warning callback for '${entry.id}'`, { error });
      }
    }
  }

  /**
   * Bind timeout to interruption state for automatic cancellation
   */
  private bindToInterruptionState(
    entry: TimeoutEntry,
    interruptionState: InterruptionStateReference,
  ): void {
    // Check if already interrupted
    if (interruptionState.shouldStop()) {
      // Cancel immediately
      const entryWithHandle = this.timeouts.get(entry.id);
      if (entryWithHandle) {
        this.cancelEntry(entryWithHandle, "interrupted");
      }
      return;
    }

    // Subscribe to resume events
    const unsubscribe = interruptionState.onResumed(() => {
      // Refresh timeout on resume
      const entryWithHandle = this.timeouts.get(entry.id);
      if (entryWithHandle && entryWithHandle.status === "active" && entryWithHandle.handle) {
        this.refresh(entryWithHandle.handle);
      }
    });

    entry.interruptionUnsubscribe = unsubscribe;
  }

  /**
   * Emit timeout event to EventRegistry
   * @param eventType Event type
   * @param payload Event payload
   */
  private emitEvent(eventType: EventType, payload: Record<string, unknown>): void {
    if (!this.eventRegistry || !this.executionId) {
      return;
    }

    try {
      const emitter = this.eventRegistry.getEmitter(this.executionId);
      if (emitter) {
        const event: BaseEvent = {
          id: `${eventType}_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
          type: eventType,
          timestamp: Date.now(),
          executionId: this.executionId,
          ...payload,
        };
        // Note: emit is async but we don't await to avoid blocking timeout operations
        emitter.emit(event).catch(error => {
          logger.warn("Failed to emit timeout event", { eventType, error });
        });
      }
    } catch (error) {
      logger.warn("Failed to emit timeout event", { eventType, error });
    }
  }
}