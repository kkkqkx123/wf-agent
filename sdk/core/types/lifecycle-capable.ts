/**
 * LifecycleCapable - A unified interface for lifecycle management
 *
 * Provides a standard lifecycle management framework for all stateful Managers.
 *
 * Key responsibilities:
 * 1. Defines standard methods for resource cleanup.
 * 2. Defines standard methods for snapshot creation and restoration.
 *
 * Design principles:
 * - Unified interface: All stateful Managers must implement this interface.
 * - Resource management: Ensures that resources are properly released.
 * - State persistence: Supports checkpoint functionality.
 *
 * Naming explanation:
 * - "Capable" indicates that the component has the capability to manage its own lifecycle.
 * - It does not manage other components; instead, it manages its own lifecycle.
 * - Clearly distinct from WorkflowLifecycleCoordinator to avoid confusion.
 */

/**
 * Lifecycle Management Capability Interface
 *
 * All stateful Managers should implement this interface to ensure:
 * 1. A unified cleanup mechanism
 * 2. A standard resource release process
 * 3. Consistent snapshot and recovery capabilities
 */
export interface LifecycleCapable<TSnapshot = unknown> {
  /**
   * Clean up resources
   *
   * Called when the manager is no longer needed, this method is used to release all occupied resources. This includes, but is not limited to:
   * - Clearing internal states
   * - Releasing memory
   * - Closing connections
   * - Canceling timers
   *
   * @throws Error An exception is thrown if the cleanup fails.
   *
   */
  cleanup(): void | Promise<void>;

  /**
   * Create a state snapshot
   *
   * Used to save a complete copy of the current state, supporting the checkpoint functionality
   *
   * @returns The state snapshot
   * @throws Error An exception is thrown if the snapshot creation fails
   */
  createSnapshot(): TSnapshot;

  /**
   * Restore from a snapshot
   *
   * Recover the state from a previously saved snapshot
   *
   * @param snapshot The state snapshot
   * @throws Error An exception is thrown if the recovery fails
   */
  restoreFromSnapshot(snapshot: TSnapshot): void | Promise<void>;
}
