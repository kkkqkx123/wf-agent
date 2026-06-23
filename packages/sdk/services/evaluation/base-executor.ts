/**
 * Base Executor
 * Abstract base class for all condition executors
 * Provides common utilities and validation
 */

import type { EvaluationContext } from "@wf-agent/types";
import type { CompiledUnit, IExecutor } from "./types/index.js";
import { resolveContextPath } from "@sdk/services/evaluation/shared/path-resolver.js";
import { getGlobalLogger } from "@wf-agent/common-utils";

export abstract class BaseExecutor implements IExecutor {
  protected logger = getGlobalLogger().child(
    this.constructor.name,
    { pkg: "sdk/workflow" },
  );

  /**
   * Execute compiled unit
   */
  abstract execute(compiled: CompiledUnit, context: EvaluationContext): unknown;

  /**
   * Validate that context has required structure
   */
  protected validateContext(context: EvaluationContext): void {
    if (!context || typeof context !== "object") {
      throw new Error("Invalid evaluation context");
    }
    if (!context.variables || typeof context.variables !== "object") {
      throw new Error("Context must have variables field");
    }
    if (!context.input || typeof context.input !== "object") {
      throw new Error("Context must have input field");
    }
    if (!context.output || typeof context.output !== "object") {
      throw new Error("Context must have output field");
    }
  }

  /**
   * Get variable value from context
   * Supports paths like "x", "input.x", "output.x", "variables.x", "items[0].name"
   */
  protected getVariableValue(path: string, context: EvaluationContext): unknown {
    return resolveContextPath(path, context);
  }
}
