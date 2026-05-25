/**
 * Condition Type Definition
 *
 * Condition and evaluation context types that are shared across
 * edges, hooks, triggers, and node configurations.
 */

import type { Metadata } from "./common.js";

/**
 * Condition interface
 * Represents a conditional expression used in workflow edges, hooks, and triggers.
 */
export interface Condition {
  /** Expression string (supports expression language syntax) */
  expression: string;
  /** Optional condition metadata */
  metadata?: Metadata;
}

/**
 * Evaluation Context Interface
 *
 * Data source access rules:
 * - variables: global/local variables, accessible via variables.xxx or simple variable names
 * - input: node input data, must be accessed explicitly via input.xxx
 * - output: node output data, must be accessed explicitly via output.xxx
 *
 * Note: To avoid data source conflicts, it is recommended to use explicit prefixes
 * (input., output., variables.).
 */
export interface EvaluationContext {
  /** Variable value mapping */
  variables: Record<string, unknown>;
  /** Input data */
  input: Record<string, unknown>;
  /** Output data */
  output: Record<string, unknown>;
  /** Allow additional properties for extensibility */
  [key: string]: unknown;
}
