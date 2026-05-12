/**
 * Variable Scope Definitions
 *
 * Simplified architecture after migration to VariableManager:
 * - Only two persistent scopes: global and execution
 * - Temporary scopes (subgraph/loop) are managed by VariableManager.scopeStack internally
 * - For checkpoint serialization, use CheckpointVariableState which includes all scopes
 */

/**
 * Variable Scope Type
 * Define the scope level of the variable in the workflow
 * 
 * Note: 'subgraph' and 'loop' are only used for temporary scope management within VariableManager.
 * They should not be accessed directly through VariableScopes interface.
 */
export type VariableScope = "global" | "execution" | "subgraph" | "loop";

/**
 * Variable Scope Structure (Runtime State)
 *
 * This interface represents the persistent variable storage structure for runtime use.
 * After migration to VariableManager, only global and execution scopes are exposed here.
 * Temporary scopes (subgraph/loop) are managed internally by VariableManager.scopeStack
 * and should be accessed via VariableManager methods (enterSubgraphScope/exitSubgraphScope).
 *
 * For checkpoint serialization, use CheckpointVariableState which captures the complete
 * variable state including temporary scopes.
 *
 * Usage:
 * - WorkflowExecution.variableScopes - runtime variable storage for execution
 * - Access only: global and execution
 * - For temporary scopes: use VariableManager methods
 *
 * Scope Characteristics:
 * - **global**: Workflow-level global variables, shared across executions with same object reference
 * - **execution**: Execution-level variables, each execution has its own, deep copied on fork
 *
 * Access Priority (from low to high):
 * global < execution < [temporary scopes managed by VariableManager]
 *
 * Description:
 * - When accessing variables, search in higher priority scopes first
 * - For persistent scopes (global/execution), access via variableScopes
 * - For temporary scopes (subgraph/loop), use VariableManager.enterSubgraphScope()/exitSubgraphScope()
 * - Usage examples:
 *   - global: Workflow configuration, constants, global state
 *   - execution: Temporary variables during workflow execution, intermediate results
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
   * Execution Scope - Within single execution
   *
   * Characteristics:
   * - Each execution has its own independent object, no interference
   * - Deep copy on fork, child execution has independent copy
   * - Modifications don't affect other executions
   * - Most commonly used variable storage location
   *
   * Example:
   * ```typescript
   * execution.variableScopes.execution['result'] = data;
   * // Only visible in this execution
   * ```
   */
  execution: Record<string, unknown>;
}
