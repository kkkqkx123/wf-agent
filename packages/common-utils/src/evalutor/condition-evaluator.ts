/**
 * ConditionEvaluator - Condition Evaluator
 * Provides a unified condition evaluator with support for expression strings
 */

import type { Condition, EvaluationContext } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { expressionEvaluator } from "./expression-evaluator.js";
import { expressionCompiler } from "./expression-compiler.js";
import { traceEvaluation, formatTrace, formatTraceAsJson, visualizeAST } from "./debug-tools.js";
import { pathExists } from "./path-resolver.js";
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
      // Pre-check: verify dependency paths exist in the appropriate sub-context.
      // Only applies when compilation succeeds; parse errors fall through to evaluate().
      try {
        const compiled = expressionCompiler.compile(condition.expression);
        const missingDeps: string[] = [];
        for (const dep of compiled.dependencies) {
          let exists = true;
          if (dep === "input" || dep === "output" || dep === "variables") {
            // Top-level context fields always exist
            exists = true;
          } else if (dep.startsWith("input.")) {
            exists = pathExists(dep.substring(6), context.input);
          } else if (dep.startsWith("output.")) {
            exists = pathExists(dep.substring(7), context.output);
          } else if (dep.startsWith("variables.")) {
            exists = pathExists(dep.substring(10), context.variables);
          } else {
            // Default: resolve against context.variables (matches expression-evaluator behavior)
            exists = pathExists(dep, context.variables);
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
      } catch {
        // Compilation failed (e.g. invalid expression). Skip pre-check and
        // let expressionEvaluator.evaluate() handle the error properly.
      }

      const result = expressionEvaluator.evaluate(condition.expression, context);
      return Boolean(result);
    } catch (error) {
      // Distinguishing between syntax errors and runtime evaluation failures
      if (error instanceof RuntimeValidationError) {
        // Syntax/parsing errors: rethrow
        throw error;
      } else {
        // Failed runtime evaluation: log AST visualization and return false
        const ast = expressionCompiler.compile(condition.expression).ast;
        const astDump = visualizeAST(ast);
        this.logger.warn(`Condition evaluation failed: ${condition.expression}`, {
          expression: condition.expression,
          astDump,
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
      const structured = formatTraceAsJson(trace);

      this.logger.info(`[DEBUG] Condition evaluation trace:\n${formatted}`, {
        expression: condition.expression,
        result: trace.result,
        totalTime: `${trace.totalTime.toFixed(2)}ms`,
        structuredTrace: structured,
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
