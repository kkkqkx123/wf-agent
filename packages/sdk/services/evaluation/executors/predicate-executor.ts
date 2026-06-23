/**
 * Predicate Condition Executor
 * Executes predicate conditions (isEmpty, isNull, isTrue, isFalse, etc.)
 */

import type { EvaluationContext } from "@wf-agent/types";
import { BaseExecutor } from "../base-executor.js";
import type { CompiledUnit, IExecutor } from "../types/index.js";

export class PredicateExecutor extends BaseExecutor implements IExecutor {
  execute(compiled: CompiledUnit, context: EvaluationContext): unknown {
    this.validateContext(context);

    const ast = compiled.ast as Record<string, unknown>;
    const type = ast['type'];
    const variable = ast['variable'] as string;

    const value = this.getVariableValue(variable, context);

    switch (type) {
      case "isEmpty":
        return this.isEmpty(value);
      case "isNotEmpty":
        return !this.isEmpty(value);
      case "isNull":
        return value === null;
      case "isNotNull":
        return value !== null;
      case "isTrue":
        return value === true;
      case "isFalse":
        return value === false;
      default:
        throw new Error(`Unknown predicate type: ${type}`);
    }
  }

  private isEmpty(value: unknown): boolean {
    if (value === null || value === undefined) {
      return true;
    }
    if (typeof value === "string") {
      return value.length === 0;
    }
    if (Array.isArray(value)) {
      return value.length === 0;
    }
    if (typeof value === "object") {
      return Object.keys(value as Record<string, unknown>).length === 0;
    }
    return false;
  }
}

export const predicateExecutor = new PredicateExecutor();
