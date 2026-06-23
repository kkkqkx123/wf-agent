/**
 * Evaluation Module
 *
 * Architecture:
 * - dsl/: Expression parsing and validation (Chevrotain-based parser)
 * - compilers/: Expression compilation to executable code
 * - executors/: Condition evaluation execution
 * - shared/: Common utilities (path resolution, validation)
 *
 * Responsibility: Evaluate conditions expressed in DSL or JavaScript
 * Does NOT include tool execution or result processing
 */

// ============================================================================
// Core Evaluation API
// ============================================================================
export { ConditionEvaluator, conditionEvaluator } from "./condition-evaluator.js";
export { CacheManager, cacheManager } from "./cache-manager.js";
export { BaseExecutor } from "./base-executor.js";

// ============================================================================
// DSL Layer (Parsing & Validation)
// ============================================================================
export {
  dslParse,
  dslParseWithErrors,
  dslValidate,
  parseToCst,
  cstToAst,
  tokenizeExpression,
} from "./dsl/index.js";

export type {
  Expression,
  LiteralExpr,
  IdentifierExpr,
  MemberAccessExpr,
  UnaryMinusExpr,
  BinaryExpr,
  NotExpr,
  TernaryExpr,
  CallExpr,
  ArrayLiteralExpr,
  NodeMetadata,
  BinaryOperator,
} from "./dsl/types.js";

// ============================================================================
// Executor Layer (Condition Evaluation)
// ============================================================================
export type { IExecutor, CompiledUnit, ICompiler } from "./types/index.js";

// ============================================================================
// Utilities (Path Resolution & Validation)
// ============================================================================
export {
  validateExpression,
  validatePath,
  validateArrayIndex,
  validateValueType,
  SECURITY_CONFIG,
  resolvePath,
  pathExists,
  setPath,
  setArrayItemByKey,
} from "@sdk/services/evaluation/shared/index.js";

export type { EvaluationContext } from "@wf-agent/types";

// ============================================================================
// Legacy Compatibility (Deprecated)
// ============================================================================
// Import for backward compatibility only
import { expressionCompiler } from "./compilers/expression-compiler.js";
import { cacheManager } from "./cache-manager.js";
import { conditionEvaluator } from "./condition-evaluator.js";
import type { EvaluationContext } from "@wf-agent/types";

/**
 * @deprecated Use ConditionEvaluator directly
 */
export class DependencyManager {
  register(key: string, expression: string, context: Record<string, unknown>) {
    const compiled = expressionCompiler.compile(expression);
    cacheManager.setCachedResult(key, undefined, compiled.dependencies ?? [], context as EvaluationContext);
    return { expression, compiled, dependencies: compiled.dependencies ?? [], lastResult: undefined };
  }

  getTrackedExpression(key: string) {
    const result = cacheManager.getCachedResult(key);
    if (result !== null) {
      return { lastResult: result };
    }
    return null;
  }

  evaluateIfChanged(key: string, context: Record<string, unknown>) {
    return conditionEvaluator.evaluate({ type: "expression", expression: key } as Record<string, unknown>, context as EvaluationContext, key);
  }

  clear() {
    cacheManager.clear();
  }
}

/**
 * @deprecated Use ConditionEvaluator directly
 */
export function createDependencyManager() {
  return new DependencyManager();
}

/**
 * @deprecated Use ConditionEvaluator directly
 */
export const expressionEvaluator = {
  evaluate: (expr: string, context: Record<string, unknown>) => {
    return conditionEvaluator.evaluate({ type: "expression", expression: expr } as Record<string, unknown>, context as EvaluationContext);
  },
  evaluateAST: (_ast: unknown, context: Record<string, unknown>) => {
    // For backward compatibility, evaluate as expression
    // Note: AST parameter ignored, evaluates expression directly
    return conditionEvaluator.evaluate({ type: "expression", expression: "" } as Record<string, unknown>, context as EvaluationContext);
  },
};
