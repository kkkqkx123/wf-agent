/**
 * Expression Evaluator Module
 * Provides functionality for parsing, evaluating conditional expressions, and path resolution.
 */

// AST type definition
export type {
  ASTNode,
  BooleanLiteralNode,
  NumberLiteralNode,
  StringLiteralNode,
  NullLiteralNode,
  ComparisonNode,
  LogicalNode,
  NotNode,
  ArithmeticNode,
  StringMethodNode,
  TernaryNode,
  ArrayMethodNode,
  ArrayMethodComparisonNode,
  FunctionCallNode,
  MemberAccessNode,
  ArrayMethodName,
  SourceLocation,
  NodeMetadata,
} from "./ast-types.js";

// Expression parser
export {
  parseValue,
  parseAST,
} from "./expression-parser.js";

// AST metadata utilities - Phase 2
export {
  extractAllMetadata,
  findNodeAtPosition,
  getNodeLocationDescription,
  extractComments,
  createMetadata,
  addComment,
} from "./ast-metadata.js";

// Expression evaluator
export { ExpressionEvaluator, expressionEvaluator } from "./expression-evaluator.js";

// Expression compiler
export { ExpressionCompiler, expressionCompiler } from "./expression-compiler.js";
export type { CompiledExpression } from "./expression-compiler.js";

// Condition evaluator
export { ConditionEvaluator, conditionEvaluator } from "./condition-evaluator.js";

// Path parser
export { resolvePath, pathExists, setPath } from "./path-resolver.js";

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

// Dependency tracking - Phase 2
export {
  DependencyManager,
  VariableChangeTracker,
  createDependencyManager,
} from "./dependency-tracker.js";
export type { TrackedExpression } from "./dependency-tracker.js";

// Debugging tools - Phase 2
export {
  visualizeAST,
  traceEvaluation,
  formatTrace,
} from "./debug-tools.js";
export type { EvaluationTrace, TraceNode } from "./debug-tools.js";
