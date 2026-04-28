/**
 * Cyclic Node Configuration Type Definition
 */

import type { Condition } from "../../graph/condition.js";

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
   * - Variable expressions: support {{variable.path}} syntax, parsed at runtime from threads and inputs
   * Example: [1,2,3] or "{{input.list}}" or "{{thread.items}}"
   */
  iterable: unknown;
  /** Loop variable name, storing the current iteration value (in loop-level scope) */
  variableName: string;
}

/**
 * Loop Start Node Configuration
 * 
 * Description: Initialize loop iteration, supports two loop modes
 * 
 * Mode 1: Data-driven loop (provide dataSource)
 * - Iterate over the specified data set (array, object, etc.)
 * - Each iteration automatically extracts the current value to the loop variable.
 * - Example: iterate through [1,2,3], each time item = current value.
 * 
 * Pattern 2: Counting loop (no dataSource provided)
 * - Fixed number of loops based on maxIterations only
 * - No loop variables, loop body can maintain its own state
 * - Example: check 10 times
 * 
 * - Loop state (iteration count, indexing, etc.) is stored in loop level scope, automatically managed with scope lifecycle.
 */
export interface LoopStartNodeConfig {
  /** Cycle ID (uniquely identifies this cycle) */
  loopId: string;
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
 * End-of-loop node configuration
 * 
 * Description: Check the loop condition and interrupt condition to decide whether to continue iteration or not.
 * - loopId uniquely identifies the loop and is used to retrieve the loop state initialized in LOOP_START.
 * - Loop state (iterable, iterationCount, etc.) is already initialized and stored in LOOP_START, no need to define it again.
 * - All loop data and state is in loop-level scopes, isolated from other scopes.
 */
export interface LoopEndNodeConfig {
  /** Loop ID (identical to LOOP_START node, used to identify and retrieve loop state) */
  loopId: string;
  /** Interrupt condition expression (optional, exits the loop as soon as it is satisfied) */
  breakCondition?: Condition;
  /** LOOP_START node ID (for jumping to the next iteration) */
  loopStartNodeId?: string;
}