/**
 * Condition Type Definition
 * Defines the types and interfaces associated with condition evaluation
 */

import type { Metadata } from "../common.js";

/**
 * conditional interface
 */
export interface Condition {
  /** Expression strings (required to support more intuitive syntax) */
  expression: string;
  /** condition metadata */
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
 * Note: To avoid data source conflicts, it is recommended to use explicit prefixes (input., output., variables.).
 */
export interface EvaluationContext {
  /** Variable Value Mapping */
  variables: Record<string, unknown>;
  /** input data */
  input: Record<string, unknown>;
  /** output data */
  output: Record<string, unknown>;
  /** Allow additional properties for extensibility */
  [key: string]: unknown;
}

/**
 * Unified variable access interface
 * Provide unified variable access methods, support nested path resolution
 */
export interface VariableAccessor {
  /**
   * Getting the value of a variable
   * @param path Variable path, supports nesting and namespaces.
   * @returns The value of the variable, or undefined if it does not exist.
   */
  get(path: string): unknown;

  /**
   * Checking if a variable exists
   * @param path Variable path
   * @returns Whether the variable exists or not
   */
  has(path: string): boolean;
}

/**
 * Conditional Evaluator Interface
 */
export interface ConditionEvaluator {
  /**
   * Evaluating conditions
   * @param condition Condition
   * @param context Evaluation context
   * @returns Whether the condition is satisfied
   */
  evaluate(condition: Condition, context: EvaluationContext): boolean;

  /**
   * Getting the value of a variable
   * @param variableName Variable name (only simple variable names are supported)
   * @param context Evaluation context
   * @returns variableValue
   */
  getVariableValue(variableName: string, context: EvaluationContext): unknown;
}
