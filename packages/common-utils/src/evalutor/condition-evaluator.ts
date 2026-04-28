/**
 * ConditionEvaluator - Condition Evaluator
 * Provides a unified condition evaluator with support for expression strings
 */

import type { Condition, EvaluationContext } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { expressionEvaluator } from "./expression-evaluator.js";
import { getGlobalLogger } from "../logger/logger.js";

/**
 * Conditional Evaluator Implementation
 */
export class ConditionEvaluator {
  private logger = getGlobalLogger().child("ConditionEvaluator", { pkg: "common-utils" });

  constructor() {
    // No initialization required, uses shared expressionEvaluator singleton
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
}

// Exporting Single Instance Examples
export const conditionEvaluator = new ConditionEvaluator();
