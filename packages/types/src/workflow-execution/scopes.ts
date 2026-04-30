/**
 * Variable Scope Definitions
 *
 * Contains two core concepts:
 * 1. VariableScope - enumerated values for a single scope
 * 2. VariableScopes - a container structure with four levels of scopes
 */

/**
 * Variable Scope Type (Single Value)
 * Define the scope level of the variable in the workflow
 */
export type VariableScope = "global" | "workflowExecution" | "local" | "loop";

/**
 * Variable Scope Structure (Runtime State)
 *
 * Usage:
 * 1. WorkflowExecution.variableScopes - runtime variable storage for execution
 * 2. ThreadStateSnapshot.variableScopes - state for checkpoint snapshots
 * 3. Runtime variable management in VariableState
 *
 * Scope Characteristics:
 * - **global**: Workflow-level global variables, shared across executions with same object reference
 * - **workflowExecution**: Execution-level variables, each execution has its own, deep copied on fork
 * - **local**: Local scope stack, supports nesting (e.g., entering subgraph)
 * - **loop**: Loop scope stack, supports nested loops
 *
 * Access Priority (from low to high):
 * global < workflowExecution < local[...] < loop[...]
 *
 * Description:
 * - When accessing variables, search in higher priority scopes first
 * - local and loop are stack structures, use top of stack (innermost) value when accessing
 * - Usage examples:
 *   - global: Workflow configuration, constants, global state
 *   - workflowExecution: Temporary variables during workflow execution, intermediate results
 *   - local: Local variables within subgraph (destroyed after subgraph ends)
 *   - loop: Loop iteration variables (destroyed after loop ends)
 */
export interface VariableScopes {
  /**
   * Global Scope - Shared across executions
   *
   * Characteristics:
   * - Set during workflow initialization
   * - All executions (including fork-created child executions) share the same object reference
   * - Modifications are visible across all executions
   * - Global variables between executions require synchronization control
   *
   * Example:
   * ```typescript
   * execution.variableScopes.global['API_KEY'] = 'xxx';
   * // Visible in other executions as well
   * ```
   */
  global: Record<string, unknown>;

  /**
   * WorkflowExecution Scope - Within single execution
   *
   * Characteristics:
   * - Each execution has its own independent object, no interference
   * - Deep copy on fork, child execution has independent copy
   * - Modifications don't affect other executions
   * - Most commonly used variable storage location
   *
   * Example:
   * ```typescript
   * execution.variableScopes.workflowExecution['result'] = data;
   * // Only visible in this execution
   * ```
   */
  workflowExecution: Record<string, unknown>;

  /**
   * Local Scope Stack - Supports nesting
   *
   * Characteristics:
   * - Array-based stack structure, each element is a scope
   * - Push new object when entering local scope (e.g., entering subgraph)
   * - Pop when exiting local scope
   * - Higher priority than global/workflowExecution scope
   * - Automatically destroyed after local scope ends, doesn't affect parent scope
   *
   * Use Cases:
   * - Local variables in subgraph
   * - Temporary calculation results
   * - Scope isolation
   *
   * Example:
   * ```typescript
   * // Enter subgraph
   * execution.variableScopes.local.push({ tempVar: 'value' });
   * // tempVar accessible in subgraph
   * // Exit subgraph
   * execution.variableScopes.local.pop();
   * // tempVar no longer visible
   * ```
   */
  local: Record<string, unknown>[];

  /**
   * Loop Scope Stack - Supports nested loops
   *
   * Characteristics:
   * - Array-based stack structure, each element is a loop's scope
   * - Push new object when entering each loop
   * - Pop when loop ends
   * - Highest priority, overrides other three scope layers
   * - Scope destroyed after loop ends
   *
   * Use Cases:
   * - Loop iteration variables (item, index, etc.)
   * - Temporary calculation results within loop
   * - Variable isolation in nested loops
   *
   * Example:
   * ```typescript
   * // Enter loop (iterating array [1, 2, 3])
   * execution.variableScopes.loop.push({ item: 1, index: 0 });
   * // item and index accessible in loop body
   * // Iteration complete, exit loop
   * execution.variableScopes.loop.pop();
   * // item and index no longer visible
   * ```
   */
  loop: Record<string, unknown>[];
}
