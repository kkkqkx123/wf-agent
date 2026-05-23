/**
 * StateManager - Common interface for all state managers
 *
 * The unified lifecycle and state management interface for all stateful managers.
 * This is the single interface that consolidates:
 * 1. Resource lifecycle management (cleanup)
 * 2. State persistence (snapshot/restore)
 * 3. Common query operations (size, isEmpty)
 * 4. Optional reset functionality
 *
 * Design principles:
 * - Minimal interface: Only includes methods that make sense for all state managers
 * - Optional methods: Uses optional methods for operations not applicable to all managers
 * - Type safety: Provides strong typing for state manager operations
 */

/**
 * Common interface for all state managers
 */
export interface StateManager<TSnapshot = unknown> {
  /**
   * Clean up resources
   *
   * Called when the manager is no longer needed, this method is used to release all
   * occupied resources. This includes, but is not limited to:
   * - Clearing internal states
   * - Releasing memory
   * - Closing connections
   * - Canceling timers
   */
  cleanup(): void | Promise<void>;

  /**
   * Create a state snapshot
   *
   * Used to save a complete copy of the current state, supporting the checkpoint functionality
   *
   * @returns The state snapshot
   */
  createSnapshot(): TSnapshot;

  /**
   * Restore from a snapshot
   *
   * Recover the state from a previously saved snapshot
   *
   * @param snapshot The state snapshot
   */
  restoreFromSnapshot(snapshot: TSnapshot): void | Promise<void>;

  /**
   * Get the number of state items managed
   * @returns Count of managed state items
   */
  size(): number;

  /**
   * Check if the state manager is empty
   * @returns true if no state is being managed
   */
  isEmpty(): boolean;

  /**
   * Reset to initial state
   * Combines cleanup and reinitialization
   * Optional because not all state managers support reset
   */
  reset?(): void | Promise<void>;
}

/**
 * Optional metadata for state managers
 * Can be used for debugging, monitoring, and diagnostics
 */
export interface StateManagerMetadata {
  /** Type of state manager (e.g., "VariableState", "ExecutionState") */
  type: string;

  /** Version for compatibility checking */
  version?: string;

  /** Creation timestamp */
  createdAt?: number;

  /** Last modification timestamp */
  lastModifiedAt?: number;

  /** Execution ID (if tied to a specific execution) */
  executionId?: string;

  /** Workflow ID (if tied to a specific workflow) */
  workflowId?: string;
}