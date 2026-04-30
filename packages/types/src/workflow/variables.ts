/**
 * Workflow Variable Type Definitions
 */

import type { VariableScope } from "../workflow-execution/scopes.js";
import type { VariableValueType } from "../workflow-execution/variables.js";

/**
 * Workflow Variable Definition Type
 * Used to declare variables at workflow definition stage, providing type safety and initial values
 *
 * Description:
 * - Declare variables at workflow definition time, providing type information and default values
 * - At execution time, converted to WorkflowExecutionVariable, stored in WorkflowExecution.variableScopes.execution
 * - Modified via VARIABLE node, accessed via expressions ({{variableName}})
 *
 * Example:
 * ```typescript
 * workflow.variables = [
 *   { name: 'userName', type: 'string', defaultValue: 'Alice' },
 *   { name: 'userAge', type: 'number', defaultValue: 25 }
 * ]
 *
 * // At execution time
 * execution.variableScopes.execution = {
 *   userName: 'Alice',
 *   userAge: 25
 * }
 *
 * // Access in expressions
 * {{userName}}  // 'Alice'
 * {{userAge}}  // 25
 * ```
 */
export interface WorkflowVariable {
  /** variable name */
  name: string;
  /** Variable type */
  type: VariableValueType;
  /** variable initial value */
  defaultValue?: unknown;
  /** Variable Description */
  description?: string;
  /** Required or not */
  required?: boolean;
  /** Read-only or not */
  readonly?: boolean;
  /** variable scope */
  scope?: VariableScope;
}
