/**
 * Checkpoint Variable State
 *
 * This type represents the complete variable state for checkpoint serialization.
 * It is separate from the runtime VariableScopes to avoid coupling between
 * runtime data structures and persistence formats.
 *
 * Design Principles:
 * - Clear separation: Runtime scopes vs. checkpoint serialization format
 * - Complete state: Includes all scopes (global, execution, and temporary scopes)
 * - Serializable: Uses plain objects/arrays for easy JSON serialization
 * - Independent evolution: Can change without affecting runtime types
 */

/**
 * Checkpoint Variable State Structure
 *
 * This structure captures the complete variable state at a point in time,
 * including both persistent scopes (global, execution).
 *
 * Usage:
 * - WorkflowExecutionStateSnapshot.variableState - checkpoint state storage
 * - Checkpoint serialization/deserialization
 *
 * Note: After architecture refactoring (Phase 1-3), scopeStack has been removed.
 * Subgraph now uses independent execution entities with their own variable managers.
 */
export interface CheckpointVariableState {
  /**
   * Global Scope Variables
   * Shared across all executions with same object reference
   */
  global: Record<string, unknown>;

  /**
   * Execution Scope Variables
   * Independent per execution, deep copied on fork
   */
  execution: Record<string, unknown>;
}
