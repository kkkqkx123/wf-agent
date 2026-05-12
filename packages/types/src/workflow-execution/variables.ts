/**
 * Workflow Execution Variable Type Definitions
 */

import type { Metadata } from "../common.js";
import type { VariableScope } from "./scopes.js";

/**
 * Variable Value Type
 */
export type VariableValueType = "number" | "string" | "boolean" | "array" | "object";

/**
 * Unified Variable Definition Type (NEW - Simplified)
 * 
 * Merges WorkflowVariable and WorkflowExecutionVariable into a single unified type.
 * This eliminates the need for type conversion and dual data structures.
 * 
 * Design Philosophy (inspired by MessageHistory):
 * - Single source of truth: One Map stores all variables
 * - Unified access interface: getVariable() / setVariable()
 * - Scope managed through metadata, not separate structures
 * 
 * Usage:
 * - Definition stage: value = defaultValue
 * - Execution stage: value = current runtime value
 * 
 * Example:
 * ```typescript
 * // Definition
 * const varDef: VariableDefinition = {
 *   name: 'counter',
 *   type: 'number',
 *   value: 0,  // Initial/default value
 *   scope: 'execution',
 *   readonly: false,
 *   metadata: { description: 'Loop counter' }
 * };
 * 
 * // During execution
 * varDef.value = 5;  // Updated value
 * ```
 */
export interface VariableDefinition {
  /** Variable name (unique identifier) */
  name: string;
  
  /** Variable type */
  type: VariableValueType;
  
  /** 
   * Variable value
   * - At definition time: serves as default/initial value
   * - At execution time: holds current runtime value
   */
  value: unknown;
  
  /** Variable scope */
  scope: VariableScope;
  
  /** Read-only flag */
  readonly: boolean;
  
  /** Optional metadata */
  metadata?: Metadata & {
    description?: string;
    required?: boolean;
  };
}

/**
 * Legacy: Workflow Execution Variable Types
 * @deprecated Use VariableDefinition instead. Will be removed after refactoring.
 */
export interface WorkflowExecutionVariable {
  /** Variable name */
  name: string;
  /** Variable value */
  value: unknown;
  /** Variable type */
  type: VariableValueType;
  /** Variable scope */
  scope: VariableScope;
  /** Read-only or not */
  readonly: boolean;
  /** Variable metadata */
  metadata?: Metadata;
}
