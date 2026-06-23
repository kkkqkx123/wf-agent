/**
 * Condition Type Definition
 *
 * Supports multiple condition types through discriminated union:
 * - Expression: String-based DSL expressions
 * - Predicate: Single-variable predicates (isEmpty, isNull, etc.)
 * - Script: Custom JavaScript code
 * - Schema: JSON Schema validation
 */

import type { Metadata } from "./common.js";

/**
 * Expression-based condition
 */
export interface ExpressionCondition {
  type: "expression";
  expression: string;
  metadata?: Metadata;
}

/**
 * Predicate condition for simple unary checks
 */
export type PredicateType =
  | "isEmpty"
  | "isNotEmpty"
  | "isNull"
  | "isNotNull"
  | "isTrue"
  | "isFalse";

export interface PredicateCondition {
  type: "predicate";
  predicateType: PredicateType;
  variable: string;
  metadata?: Metadata;
}

/**
 * Script-based condition with custom JavaScript
 */
export interface ScriptCondition {
  type: "script";
  script: string;
  metadata?: Metadata;
}

/**
 * JSON Schema-based validation condition
 */
export interface SchemaCondition {
  type: "schema";
  variable: string;
  schema: Record<string, unknown>;
  metadata?: Metadata;
}

/**
 * Union type for all condition types
 */
export type Condition = ExpressionCondition | PredicateCondition | ScriptCondition | SchemaCondition;

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
