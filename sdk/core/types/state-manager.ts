/**
 * StateManager - Common interface for all state managers
 *
 * Extends LifecycleCapable with additional common operations to provide
 * a unified API for state management across the workflow system.
 *
 * Key responsibilities:
 * 1. Provides standard lifecycle management (via LifecycleCapable)
 * 2. Offers common query operations (size, isEmpty)
 * 3. Supports optional reset functionality
 *
 * Design principles:
 * - Minimal interface: Only includes methods that make sense for all state managers
 * - Optional methods: Uses optional methods for operations not applicable to all managers
 * - Type safety: Provides strong typing for state manager operations
 */

import type { LifecycleCapable } from "./lifecycle-capable.js";

/**
 * Common interface for all state managers
 * Extends LifecycleCapable with additional common operations
 */
export interface StateManager<TSnapshot = unknown> extends LifecycleCapable<TSnapshot> {
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
