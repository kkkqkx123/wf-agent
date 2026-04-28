/**
 * Thread Variable Type Definitions
 */

import type { Metadata } from "../common.js";
import type { VariableScope } from "./scopes.js";

/**
 * Variable Value Type
 */
export type VariableValueType = "number" | "string" | "boolean" | "array" | "object";

/**
 * Thread Variable Types
 */
export interface ThreadVariable {
  /** variable name */
  name: string;
  /** variable value */
  value: unknown;
  /** Variable type */
  type: VariableValueType;
  /** variable scope */
  scope: VariableScope;
  /** Read-only or not */
  readonly: boolean;
  /** variable metadata */
  metadata?: Metadata;
}
