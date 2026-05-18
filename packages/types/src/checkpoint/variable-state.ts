/**
 * Checkpoint Variable State
 *
 * This type represents the complete variable state for checkpoint serialization.
 * It is separate from runtime data structures to avoid coupling between
 * runtime and persistence formats.
 *
 * Design Principles:
 * - Clear separation: Runtime vs. checkpoint serialization format
 * - Complete state: All variables in a flat structure
 * - Serializable: Uses plain objects/arrays for easy JSON serialization
 * - Independent evolution: Can change without affecting runtime types
 */

/**
 * Checkpoint Variable State Structure
 *
 * This structure captures the complete variable state at a point in time.
 * All variables are stored in a single flat map.
 *
 * Usage:
 * - WorkflowExecutionStateSnapshot.variableState - checkpoint state storage
 * - Checkpoint serialization/deserialization
 */
export interface CheckpointVariableState {
  /**
   * All Variables (Flat Structure)
   * No scope distinction - all variables managed uniformly
   */
  variables: Record<string, unknown>;
}
