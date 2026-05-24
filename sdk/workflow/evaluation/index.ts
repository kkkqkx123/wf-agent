/**
 * Evaluation Module
 * Provides expression evaluation, condition evaluation, and dependency tracking.
 */

// Expression evaluator
export { ExpressionEvaluator, expressionEvaluator } from "./expression-evaluator.js";

// Expression compiler
export { ExpressionCompiler, expressionCompiler } from "./expression-compiler.js";
export type { CompiledExpression } from "./expression-compiler.js";

// Condition evaluator
export { ConditionEvaluator, conditionEvaluator } from "./condition-evaluator.js";

// Security validator
export {
  validateExpression,
  validatePath,
  validateArrayIndex,
  validateValueType,
  SECURITY_CONFIG,
} from "./security-validator.js";

// Path resolver
export { resolvePath, pathExists, setPath, setArrayItemByKey } from "./path-resolver.js";

// Dependency tracking
export {
  DependencyManager,
  createDependencyManager,
} from "./dependency-tracker.js";
export type { TrackedExpression } from "./dependency-tracker.js";

// DSL types
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
