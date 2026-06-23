/**
 * Condition Evaluator - Unified Condition Dispatch and Evaluation
 * Routes conditions to appropriate evaluators and manages caching
 */

import type { Condition, EvaluationContext } from "@wf-agent/types";
import { ExpressionSecurityError } from "@wf-agent/types";
import { getGlobalLogger } from "@wf-agent/common-utils";
import { cacheManager } from "./cache-manager.js";
import { expressionCompiler } from "./compilers/expression-compiler.js";
import { expressionConditionExecutor } from "./executors/expression-condition-executor.js";
import { predicateCompiler } from "./compilers/predicate-compiler.js";
import { predicateExecutor } from "./executors/predicate-executor.js";
import { schemaCompiler } from "./compilers/schema-compiler.js";
import { schemaExecutor } from "./executors/schema-executor.js";
import { scriptCompiler } from "./compilers/script-compiler.js";
import { scriptExecutor } from "./executors/script-executor.js";

/**
 * Custom error types for proper error handling
 */
class EvaluationError extends Error {
  constructor(message: string, public readonly code: string) {
    super(message);
  }
}

/**
 * Unified Condition Evaluator
 * Handles all condition types through a dispatcher pattern
 * Issue 7: Implements safe condition type detection with validation
 */
export class ConditionEvaluator {
  private logger = getGlobalLogger().child("ConditionEvaluator", { pkg: "sdk/workflow" });

  /**
   * Validate condition structure and required fields
   * Issue 7: Ensures conditions have correct type and all required fields
   *
   * @param condition Condition to validate
   * @throws TypeError if condition structure is invalid
   */
  private validateCondition(condition: unknown): void {
    if (!condition || typeof condition !== "object") {
      throw new TypeError("Condition must be an object");
    }

    const obj = condition as Record<string, unknown>;
    const type = (obj['type'] as string) ?? "expression";

    switch (type) {
      case "expression": {
        if (typeof obj['expression'] !== "string") {
          throw new TypeError("expression condition requires 'expression' field of type string");
        }
        if ((obj['expression'] as string).trim().length === 0) {
          throw new TypeError("expression condition requires non-empty expression string");
        }
        break;
      }

      case "predicate": {
        if (typeof obj['predicateType'] !== "string") {
          throw new TypeError("predicate condition requires 'predicateType' field of type string");
        }
        if (typeof obj['variable'] !== "string") {
          throw new TypeError("predicate condition requires 'variable' field of type string");
        }
        const validTypes = ["isEmpty", "isNotEmpty", "isNull", "isNotNull", "isTrue", "isFalse"];
        if (!validTypes.includes(obj['predicateType'] as string)) {
          throw new TypeError(`predicate condition has invalid predicateType: ${obj['predicateType']}. Must be one of: ${validTypes.join(", ")}`);
        }
        break;
      }

      case "schema": {
        if (typeof obj['variable'] !== "string") {
          throw new TypeError("schema condition requires 'variable' field of type string");
        }
        if (!obj['schema'] || typeof obj['schema'] !== "object") {
          throw new TypeError("schema condition requires 'schema' field of type object");
        }
        break;
      }

      case "script": {
        if (typeof obj['script'] !== "string") {
          throw new TypeError("script condition requires 'script' field of type string");
        }
        if ((obj['script'] as string).trim().length === 0) {
          throw new TypeError("script condition requires non-empty script string");
        }
        break;
      }

      default:
        throw new TypeError(`Unknown condition type: ${type}. Must be one of: expression, predicate, schema, script`);
    }
  }

  /**
   * Evaluate a condition against a context
   * Routes to appropriate compiler/executor based on condition type
   * Integrates with unified cache manager
   *
   * @param condition Condition to evaluate
   * @param context Evaluation context
   * @param cacheKey Optional cache key for result caching
   * @returns Evaluation result as boolean
   * @throws TypeError if condition structure is invalid
   * @throws ExpressionSecurityError if security validation fails
   */
  evaluate(condition: Condition | Record<string, unknown>, context: EvaluationContext, cacheKey?: string): boolean {
    // Issue 7: Validate condition structure before processing
    this.validateCondition(condition);

    const conditionType = ((condition as Record<string, unknown>)['type'] as string) ?? "expression";

    // Check result cache if key provided
    if (cacheKey) {
      if (!cacheManager.hasDependenciesChanged(cacheKey, context)) {
        const cached = cacheManager.getCachedResult(cacheKey);
        if (cached !== null) {
          return Boolean(cached);
        }
      }
    }

    try {
      let result: boolean;

      switch (conditionType) {
        case "expression": {
          const expr = condition as Record<string, unknown>;
          const compileCacheKey = cacheManager.generateCompilationCacheKey("expression", expr['expression'] as string);
          let compiled = cacheManager.getCompiled(compileCacheKey);
          if (!compiled) {
            compiled = expressionCompiler.compile(expr['expression'] as string);
            cacheManager.setCompiled(compileCacheKey, compiled);
          }

          const execResult = expressionConditionExecutor.execute(compiled, context);
          result = Boolean(execResult);
          break;
        }

        case "predicate": {
          const pred = condition as Record<string, unknown>;
          const compileCacheKey = cacheManager.generateCompilationCacheKey("predicate", {
            predicateType: pred['predicateType'],
            variable: pred['variable'],
          });
          let compiled = cacheManager.getCompiled(compileCacheKey);
          if (!compiled) {
            compiled = predicateCompiler.compile({
              type: pred['predicateType'] as string,
              variable: pred['variable'] as string,
            });
            cacheManager.setCompiled(compileCacheKey, compiled);
          }

          const execResult = predicateExecutor.execute(compiled, context);
          result = Boolean(execResult);
          break;
        }

        case "schema": {
          const sch = condition as Record<string, unknown>;
          const compileCacheKey = cacheManager.generateCompilationCacheKey("schema", {
            variable: sch['variable'],
            schema: sch['schema'],
          });
          let compiled = cacheManager.getCompiled(compileCacheKey);
          if (!compiled) {
            compiled = schemaCompiler.compile(sch['schema'] as string | Record<string, unknown>);
            cacheManager.setCompiled(compileCacheKey, compiled);
          }

          const execResult = schemaExecutor.execute(compiled, context, sch['variable'] as string);
          result = Boolean(execResult);
          break;
        }

        case "script": {
          const scr = condition as Record<string, unknown>;
          const compileCacheKey = cacheManager.generateCompilationCacheKey("script", scr['script'] as string);
          let compiled = cacheManager.getCompiled(compileCacheKey);
          if (!compiled) {
            compiled = scriptCompiler.compile(scr['script'] as string);
            cacheManager.setCompiled(compileCacheKey, compiled);
          }

          const execResult = scriptExecutor.execute(compiled, context);
          result = Boolean(execResult);
          break;
        }

        default:
          throw new Error(`Unknown condition type: ${conditionType}`);
      }

      // Cache result if key provided
      // Note: script type is not cached because dependencies cannot be statically analyzed
      if (cacheKey && conditionType !== "script") {
        const deps = this.extractDependencies(condition as Record<string, unknown>);
        cacheManager.setCachedResult(cacheKey, result, deps, context);
      }

      return result;
    } catch (error) {
      // Use proper error type checking instead of string matching
      this.handleEvaluationError(error, conditionType);
      return false;
    }
  }

  /**
   * Handle evaluation errors with proper type checking
   */
  private handleEvaluationError(error: unknown, conditionType: string): void {
    // Always propagate security errors
    if (error instanceof ExpressionSecurityError) {
      this.logger.warn(`Security validation failed for ${conditionType} condition`, {
        type: conditionType,
        error: error.message,
      });
      throw error;
    }

    // Always propagate validation errors
    if (error instanceof TypeError) {
      this.logger.warn(`Validation error for ${conditionType} condition`, {
        type: conditionType,
        error: error.message,
      });
      throw error;
    }

    // Propagate evaluation errors (compilation, runtime)
    if (error instanceof EvaluationError) {
      this.logger.warn(`Evaluation failed for ${conditionType} condition`, {
        type: conditionType,
        code: error.code,
        error: error.message,
      });
      throw error;
    }

    // Log other errors but don't re-throw
    this.logger.warn(`Unexpected error evaluating ${conditionType} condition`, {
      type: conditionType,
      error: error instanceof Error ? error.message : String(error),
    });
  }

  /**
   * Extract dependencies from a condition for caching
   */
  private extractDependencies(condition: Record<string, unknown>): string[] {
    const type = condition['type'] ?? "expression";

    switch (type) {
      case "expression": {
        try {
          const compiled = expressionCompiler.compile(condition['expression'] as string);
          return compiled.dependencies ?? [];
        } catch {
          return [];
        }
      }

      case "predicate": {
        return [condition['variable'] as string].filter(Boolean);
      }

      case "schema": {
        return [condition['variable'] as string].filter(Boolean);
      }

      case "script":
        return [];

      default:
        return [];
    }
  }
}

export const conditionEvaluator = new ConditionEvaluator();
