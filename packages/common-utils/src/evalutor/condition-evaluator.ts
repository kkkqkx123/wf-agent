/**
 * ConditionEvaluator - Condition Evaluator
 * Provides a unified condition evaluator with support for expression strings
 */

import type { Condition, EvaluationContext } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { expressionEvaluator } from "./expression-evaluator.js";
import { traceEvaluation, formatTrace } from "./debug-tools.js";
import { getGlobalLogger } from "../logger/index.js";

/**
 * Conditional Evaluator Implementation
 */
export class ConditionEvaluator {
  private logger = getGlobalLogger().child("ConditionEvaluator", { pkg: "common-utils" });
  private debugMode = false;

  constructor(debug?: boolean) {
    if (debug) {
      this.enableDebug();
    }
  }

  /**
   * Enable debug mode - uses traceEvaluation to capture detailed evaluation traces
   */
  enableDebug(): void {
    this.debugMode = true;
  }

  /**
   * Disable debug mode
   */
  disableDebug(): void {
    this.debugMode = false;
  }

  /**
   * Check if debug mode is enabled
   */
  isDebugEnabled(): boolean {
    return this.debugMode;
  }

  /**
   * Evaluating conditions
   * @param condition Condition
   * @param context Evaluation context
   * @returns Whether the condition is satisfied
   */
  evaluate(condition: Condition, context: EvaluationContext): boolean {
    // The expression field must be provided
    if (!condition.expression) {
      throw new RuntimeValidationError("Condition must have an expression field", {
        operation: "condition_evaluation",
        context: { condition },
      });
    }

    // Debug mode: capture detailed evaluation trace
    if (this.debugMode) {
      return this.evaluateWithTrace(condition, context);
    }

    try {
      const result = expressionEvaluator.evaluate(condition.expression, context);
      return Boolean(result);
    } catch (error) {
      // Distinguishing between syntax errors and runtime evaluation failures
      if (error instanceof RuntimeValidationError) {
        // Syntax/parsing errors: rethrow
        throw error;
      } else {
        // Failed runtime evaluation: log and return false
        this.logger.warn(`Condition evaluation failed: ${condition.expression}`, {
          expression: condition.expression,
          error: error instanceof Error ? error.message : String(error),
        });
        return false;
      }
    }
  }

  /**
   * Evaluate condition with debug tracing
   */
  private evaluateWithTrace(condition: Condition, context: EvaluationContext): boolean {
    try {
      const trace = traceEvaluation(condition.expression, context);
      const formatted = formatTrace(trace);

      this.logger.info(`[DEBUG] Condition evaluation trace:\n${formatted}`, {
        expression: condition.expression,
        result: trace.result,
        totalTime: `${trace.totalTime.toFixed(2)}ms`,
      });

      return Boolean(trace.result);
    } catch (error) {
      this.logger.warn(`[DEBUG] Condition evaluation failed: ${condition.expression}`, {
        expression: condition.expression,
        error: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
  }
}

// Exporting Single Instance Examples
export const conditionEvaluator = new ConditionEvaluator();
