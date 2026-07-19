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
// Compilers
// ============================================================================
export { scriptCompiler } from "./compilers/script-compiler.js";
export { expressionCompiler } from "./compilers/expression-compiler.js";

// ============================================================================
// Executors
// ============================================================================
export { ExpressionConditionExecutor, expressionConditionExecutor } from "./executors/expression-condition-executor.js";

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
