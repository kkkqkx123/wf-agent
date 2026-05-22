/**
 * Cyclic Node Configuration Type Definition
 * 
 * Design Philosophy:
 * - Explicit variable passing (no implicit scope inheritance)
 * - Loop variables must be declared in LOOP_START
 * - Clear data flow between loop iterations and parent workflow
 */

import type { Condition } from "../../graph/condition.js";

/**
 * Loop Variable Input Mapping
 * Defines which parent workflow variables are accessible within the loop
 */
export interface LoopVariableInput {
  /** Parent workflow variable name (source) */
  externalName: string;
  
  /** Loop internal variable name (target) */
  internalName: string;
  
  /** Whether this input is required */
  required?: boolean;
  
  /** Default value if parent variable is not found */
  defaultValue?: unknown;
  
  /** Description for documentation */
  description?: string;
}

/**
 * Cyclic Data Source Configuration
 * 
 * Description: Defines the data source and loop variables for loop iteration.
 * - iterable: the data source to be iterated (array, object, number, string or variable expression).
 * - variableName: the name of the loop variable that stores the current iteration value.
 * - Both attributes must be present or absent (used in pairs).
 */
export interface DataSource {
  /** Iterable objects or variable expressions
   * - Direct values: arrays, objects, numbers, strings
   * - Variable expressions: support {{variable.path}} syntax, parsed at runtime from workflowExecutions and inputs
   * Example: [1,2,3] or "{{input.list}}" or "{{workflowExecution.items}}"
   */
  iterable: unknown;
  /** Loop variable name, storing the current iteration value */
  variableName: string;
}

/**
 * Loop Start Node Output
 * - loopId: string - The loop identifier
 * - iterationCount: number - Current iteration number
 * - maxIterations: number - Maximum allowed iterations
 * - hasMoreIterations: boolean - Whether more iterations remain
 */
export interface LoopStartNodeOutput {
  loopId: string;
  iterationCount: number;
  maxIterations: number;
  hasMoreIterations: boolean;
}

/**
 * Loop Start Node Configuration
 *
 * Description: Initialize loop iteration with explicit variable mapping.
 *
 * IMPORTANT: Loops do NOT inherit parent workflow variables automatically.
 * All variables needed inside the loop must be explicitly declared in variableInputs.
 *
 * Mode 1: Data-driven loop (provide dataSource)
 * - Iterate over the specified data set (array, object, etc.)
 * - Each iteration automatically extracts the current value to the loop variable.
 * - Example: iterate through [1,2,3], each time item = current value.
 *
 * Mode 2: Counting loop (no dataSource provided)
 * - Fixed number of loops based on maxIterations only
 * - No loop variables, loop body can maintain its own state
 * - Example: check 10 times
 */
export interface LoopStartNodeConfig {
  /** Cycle ID (uniquely identifies this cycle) */
  loopId: string;
  
  /**
   * Variable Input Mapping
   *
   * Explicitly defines which parent workflow variables are accessible within the loop.
   * This is the ONLY way for loop body to access parent variables.
   *
   * Example:
   * ```typescript
   * variableInputs: [
   *   {
   *     externalName: "config",        // Parent's variable
   *     internalName: "loopConfig",    // Loop sees it as this name
   *     required: true,
   *     description: "Configuration for loop processing"
   *   },
   *   {
   *     externalName: "threshold",
   *     internalName: "maxValue",
   *     defaultValue: 100
   *   }
   * ]
   * ```
   */
  variableInputs?: LoopVariableInput[];
  
  /** Data source configuration (optional)
   * - When provided: data-driven loop, traversing dataSource.iterable
   * - When not provided: a counting loop, based on maxIterations only.
   * - If provided, both iterable and variableName must exist.
   */
  dataSource?: DataSource;
  
  /** Maximum number of iterations (safety protection, required) */
  maxIterations: number;
}

/**
 * Loop End Node Output
 * - loopId: string - The loop identifier
 * - breakTriggered: boolean - Whether the break condition was met
 * - iterationCount: number - Current iteration number
 * - nextIteration: boolean - Whether to proceed to next iteration
 */
export interface LoopEndNodeOutput {
  loopId: string;
  breakTriggered: boolean;
  iterationCount: number;
  nextIteration: boolean;
}

/**
 * End-of-loop node configuration
 *
 * Description: Check the loop condition and interrupt condition to decide whether to continue iteration or not.
 * - loopId uniquely identifies the loop and is used to retrieve the loop state initialized in LOOP_START.
 * - Loop state (iterable, iterationCount, etc.) is already initialized and stored in LOOP_START, no need to define it again.
 */
export interface LoopEndNodeConfig {
  /** Loop ID (identical to LOOP_START node, used to identify and retrieve loop state) */
  loopId: string;
  /** Interrupt condition expression (optional, exits the loop as soon as it is satisfied) */
  breakCondition?: Condition;
  /** LOOP_START node ID (for jumping to the next iteration) */
  loopStartNodeId?: string;
}