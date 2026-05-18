/**
 * Workflow Execution Variable Type Definitions
 */

import type { Metadata } from "../common.js";

/**
 * Variable Value Type
 */
export type VariableValueType = "number" | "string" | "boolean" | "array" | "object";

/**
 * Unified Variable Definition Type (SIMPLIFIED)
 * 
 * Single variable definition without scope concept.
 * All variables are stored in a flat structure managed by VariableManager.
 * Cross-boundary variable passing is done through explicit importVariables/exportVariables API.
 * 
 * Design Philosophy:
 * - No implicit scope inheritance
 * - Explicit data flow at boundaries (SUBGRAPH, LOOP, FORK)
 * - Deep clone on import/export to ensure isolation
 * 
 * Usage:
 * ```typescript
 * const varDef: VariableDefinition = {
 *   name: 'counter',
 *   type: 'number',
 *   value: 0,
 *   readonly: false,
 *   metadata: { description: 'Loop counter' }
 * };
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
  
  /** Read-only flag - prevents reassignment of the variable */
  readonly: boolean;
  
  /** 
   * Freeze flag - prevents mutation of object/array values
   * When true, object values are frozen using Object.freeze() during registration
   * Note: This is shallow freeze - nested objects are not frozen
   * Default: false
   */
  freeze?: boolean;
  
  /** Optional metadata */
  metadata?: Metadata & {
    description?: string;
    required?: boolean;
  };
}
