/**
 * Executor Interface and Types
 * Defines the contract for all condition executors
 */

import type { EvaluationContext } from "@wf-agent/types";
import type { CompiledUnit } from "./compiler.js";

export interface IExecutor {
  /**
   * Execute compiled unit against context
   * @param compiled Compiled unit from compiler
   * @param context Evaluation context with variables/input/output
   * @returns Evaluation result
   */
  execute(compiled: CompiledUnit, context: EvaluationContext): unknown;
}
