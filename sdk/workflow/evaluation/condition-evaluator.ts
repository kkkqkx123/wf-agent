/**
 * ConditionEvaluator - Condition Evaluator
 * Provides a unified condition evaluator with support for expression strings
 */

import type { Condition, EvaluationContext } from "@wf-agent/types";
import { ExpressionSecurityError, RuntimeValidationError } from "@wf-agent/types";
import { getGlobalLogger } from "@wf-agent/common-utils";
import { expressionEvaluator } from "./expression-evaluator.js";
import { expressionCompiler } from "./expression-compiler.js";
import { resolvePath } from "./path-resolver.js";

/**
 * Conditional Evaluator Implementation
 */
export class ConditionEvaluator {
  private logger = getGlobalLogger().child("ConditionEvaluator", { pkg: "sdk/workflow" });

  /**
   * Evaluating conditions
   * @param condition Condition
   * @param context Evaluation context
   * @returns Whether the condition is satisfied
   */
  evaluate(condition: Condition, context: EvaluationContext): boolean {
    // The expression field must be provided
    if (!condition.expression) {
      throw new ExpressionSecurityError("Condition must have an expression field", {
        operation: "condition_evaluation",
        context: { condition },
      });
    }

    try {
      // Pre-check: verify dependency paths exist in the appropriate sub-context.
      // Only applies when compilation succeeds; parse errors fall through to evaluate().
      let compiled: CompiledExpression | null = null;
      try {
        compiled = expressionCompiler.compile(condition.expression);
      } catch {
        // Compilation failed (e.g. invalid expression). Skip pre-check and
        // let expressionEvaluator.evaluate() handle the error properly.
      }

      // Dependency path validation (separate from compilation catch so security errors propagate)
      if (compiled) {
        const missingDeps: string[] = [];
        for (const dep of compiled.dependencies) {
          let exists = true;
          if (dep === "input" || dep === "output" || dep === "variables") {
            // Top-level context fields always exist
            exists = true;
          } else if (dep.startsWith("input.")) {
            try {
              exists = resolvePath(dep.substring(6), context.input) !== undefined;
            } catch (e) {
              if (e instanceof ExpressionSecurityError) throw e;
              exists = false;
            }
          } else if (dep.startsWith("output.")) {
            try {
              exists = resolvePath(dep.substring(7), context.output) !== undefined;
            } catch (e) {
              if (e instanceof ExpressionSecurityError) throw e;
              exists = false;
            }
          } else if (dep.startsWith("variables.")) {
            try {
              exists = resolvePath(dep.substring(10), context.variables) !== undefined;
            } catch (e) {
              if (e instanceof ExpressionSecurityError) throw e;
              exists = false;
            }
          } else {
            // Default: resolve against context.variables (matches expression-evaluator behavior)
            try {
              exists = resolvePath(dep, context.variables) !== undefined;
            } catch (e) {
              if (e instanceof ExpressionSecurityError) throw e;
              exists = false;
            }
          }
          if (!exists) {
            missingDeps.push(dep);
          }
        }
        if (missingDeps.length > 0) {
          this.logger.warn(`Condition dependencies missing in context: ${missingDeps.join(", ")}`, {
            expression: condition.expression,
            missingDependencies: missingDeps,
          });
          // Missing dependencies evaluate as false, return early
          return false;
        }
      }

      const result = expressionEvaluator.evaluate(condition.expression, context);
      return Boolean(result);
    } catch (error) {
      // Distinguishing between security errors and runtime evaluation failures
      if (error instanceof ExpressionSecurityError) {
        // Security/expression errors: rethrow
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
