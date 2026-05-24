/**
 * Expression Evaluator Module
 * Provides functionality for parsing, evaluating conditional expressions, and path resolution.
 */

// DSL types
export type { Expression, NodeMetadata } from "./dsl/types.js";

// Expression evaluator
export { ExpressionEvaluator, expressionEvaluator } from "./expression-evaluator.js";

// Expression compiler
export { ExpressionCompiler, expressionCompiler } from "./expression-compiler.js";
export type { CompiledExpression } from "./expression-compiler.js";

// Condition evaluator
export { ConditionEvaluator, conditionEvaluator } from "./condition-evaluator.js";

// Path parser
export { resolvePath, pathExists, setPath, setArrayItemByKey } from "./path-resolver.js";

// Security Validator
export {
  validateExpression,
  validatePath,
  validateArrayIndex,
  validateValueType,
  SECURITY_CONFIG,
} from "./security-validator.js";

// Type Validator
export {
  validateComparisonTypes,
  validateArrayMethodResult,
  isValidNumber,
  isArray,
} from "./type-validator.js";

// Dependency tracking
export {
  DependencyManager,
  VariableChangeTracker,
  createDependencyManager,
} from "./dependency-tracker.js";
export type { TrackedExpression } from "./dependency-tracker.js";

// Debugging tools
export {
  visualizeAST,
  traceEvaluation,
  formatTrace,
  formatTraceAsJson,
} from "./debug-tools.js";
export type { EvaluationTrace, TraceNode, StructuredTraceMetadata } from "./debug-tools.js";

// AST metadata utilities
export {
  extractAllMetadata,
  findNodeAtPosition,
  getNodeLocationDescription,
  extractComments,
  createMetadata,
  addComment,
} from "./ast-metadata.js";

// DSL parser utilities
export { dslParse, dslValidate, tokenizeExpression } from "./dsl/index.js";