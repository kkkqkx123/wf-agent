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
} from "./ast-types.js";

// Expression parser
export {
  parseValue,
  parseAST,
} from "./expression-parser.js";

// Expression evaluator
export { ExpressionEvaluator, expressionEvaluator } from "./expression-evaluator.js";

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
