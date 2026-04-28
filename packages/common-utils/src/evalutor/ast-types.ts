/**
 * AST Type Definitions
 * Defines all node types of the expression abstract syntax tree
 */

/**
 * AST Node Base Type
 */
export type ASTNode =
  | BooleanLiteralNode
  | NumberLiteralNode
  | StringLiteralNode
  | NullLiteralNode
  | ComparisonNode
  | LogicalNode
  | NotNode
  | ArithmeticNode
  | StringMethodNode
  | TernaryNode;

/**
 * Boolean Literals node
 */
export interface BooleanLiteralNode {
  type: "boolean";
  value: boolean;
}

/**
 * Numeric literal node
 */
export interface NumberLiteralNode {
  type: "number";
  value: number;
}

/**
 * string literal node
 */
export interface StringLiteralNode {
  type: "string";
  value: string;
}

/**
 * null literal node
 */
export interface NullLiteralNode {
  type: "null";
  value: null;
}

/**
 * Compare operation nodes
 */
export interface ComparisonNode {
  type: "comparison";
  variablePath: string;
  operator: "==" | "!=" | ">" | "<" | ">=" | "<=" | "contains" | "in";
  value: unknown;
}

/**
 * Logical operation nodes
 */
export interface LogicalNode {
  type: "logical";
  operator: "&&" | "||";
  left: ASTNode;
  right: ASTNode;
}

/**
 * NOT operation node
 */
export interface NotNode {
  type: "not";
  operand: ASTNode;
}

/**
 * Arithmetic operation node
 */
export interface ArithmeticNode {
  type: "arithmetic";
  operator: "+" | "-" | "*" | "/" | "%";
  left: ASTNode;
  right: ASTNode;
}

/**
 * String method node
 */
export interface StringMethodNode {
  type: "stringMethod";
  method: "startsWith" | "endsWith" | "length" | "toLowerCase" | "toUpperCase" | "trim";
  variablePath: string;
  argument?: unknown;
}

/**
 * Ternary Operator Node
 */
export interface TernaryNode {
  type: "ternary";
  condition: ASTNode;
  consequent: ASTNode;
  alternate: ASTNode;
}
