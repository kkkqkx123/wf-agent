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
 * Workflow Execution Variable Types
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
