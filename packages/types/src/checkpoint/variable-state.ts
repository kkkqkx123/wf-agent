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
 * including both persistent scopes (global, execution) and temporary scopes
 * (subgraph/loop managed via scopeStack).
 *
 * Usage:
 * - WorkflowExecutionStateSnapshot.variableState - checkpoint state storage
 * - Checkpoint serialization/deserialization
 *
 * Note: This is different from VariableScopes which only contains persistent scopes.
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

  /**
   * Temporary Scope Stack
   * Contains all nested temporary scopes (subgraph, loop, etc.)
   * Each entry represents one level of scope nesting
   *
   * Note: After migration to VariableManager, subgraph and loop scopes
   * are unified in this stack. The distinction is no longer maintained.
   */
  scopeStack: Record<string, unknown>[];
}
