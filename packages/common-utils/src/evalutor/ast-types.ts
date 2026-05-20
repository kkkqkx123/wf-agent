/**
 * AST Type Definitions
 * Defines all node types of the expression abstract syntax tree
 */

/**
 * Source Location Information
 * Represents the position of a node in the original expression string
 */
export interface SourceLocation {
  /** Start position (0-based index) */
  start: number;
  /** End position (0-based index, exclusive) */
  end: number;
  /** Line number (1-based) */
  line?: number;
  /** Column number (1-based) */
  column?: number;
}

/**
 * Metadata for AST nodes
 * Contains additional information like comments and source location
 */
export interface NodeMetadata {
  /** Source code location */
  location?: SourceLocation;
  /** Comments associated with this node */
  comments?: string[];
  /** Custom metadata (extensible) */
  custom?: Record<string, unknown>;
}

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
  | TernaryNode
  | ArrayMethodNode
  | ArrayMethodComparisonNode
  | FunctionCallNode
  | MemberAccessNode;

/**
 * Boolean Literals node
 */
export interface BooleanLiteralNode {
  type: "boolean";
  value: boolean;
  metadata?: NodeMetadata;
}

/**
 * Numeric literal node
 */
export interface NumberLiteralNode {
  type: "number";
  value: number;
  metadata?: NodeMetadata;
}

/**
 * string literal node
 */
export interface StringLiteralNode {
  type: "string";
  value: string;
  metadata?: NodeMetadata;
}

/**
 * null literal node
 */
export interface NullLiteralNode {
  type: "null";
  value: null;
  metadata?: NodeMetadata;
}

/**
 * Compare operation nodes
 */
export interface ComparisonNode {
  type: "comparison";
  variablePath: string;
  operator: "==" | "!=" | ">" | "<" | ">=" | "<=" | "contains" | "in";
  value: unknown;
  metadata?: NodeMetadata;
}

/**
 * Logical operation nodes
 */
export interface LogicalNode {
  type: "logical";
  operator: "&&" | "||";
  left: ASTNode;
  right: ASTNode;
  metadata?: NodeMetadata;
}

/**
 * NOT operation node
 */
export interface NotNode {
  type: "not";
  operand: ASTNode;
  metadata?: NodeMetadata;
}

/**
 * Arithmetic operation node
 */
export interface ArithmeticNode {
  type: "arithmetic";
  operator: "+" | "-" | "*" | "/" | "%";
  left: ASTNode;
  right: ASTNode;
  metadata?: NodeMetadata;
}

/**
 * String method node
 */
export interface StringMethodNode {
  type: "stringMethod";
  method: "startsWith" | "endsWith" | "length" | "toLowerCase" | "toUpperCase" | "trim";
  variablePath: string;
  argument?: unknown;
  metadata?: NodeMetadata;
}

/**
 * Ternary Operator Node
 */
export interface TernaryNode {
  type: "ternary";
  condition: ASTNode;
  consequent: ASTNode;
  alternate: ASTNode;
  metadata?: NodeMetadata;
}

/**
 * Array Method Node
 */
export interface ArrayMethodNode {
  type: "arrayMethod";
  method: ArrayMethodName;
  arrayPath: string;
  propertyName: string;
  value?: unknown;
  metadata?: NodeMetadata;
}

/**
 * Array Method Comparison Node
 * Represents comparison operations with array method results
 * Example: input.messages.countWhere('role', 'user') > 5
 */
export interface ArrayMethodComparisonNode {
  type: "arrayMethodComparison";
  methodNode: ArrayMethodNode;
  operator: "==" | "!=" | ">" | "<" | ">=" | "<=";
  compareValue: unknown;
  metadata?: NodeMetadata;
}

/**
 * Array Method Names
 */
export type ArrayMethodName =
  | "someEqual"
  | "someContains"
  | "everyEqual"
  | "everyHas"
  | "countWhere"
  | "countWhereContains"
  | "findEqual"
  | "findContains"
  | "has"
  | "hasContains"
  // Aggregation functions (Phase 3.1)
  | "sum"
  | "avg"
  | "min"
  | "max"
  // Comparison-based filters (Phase 3.2)
  | "someGreaterThan"
  | "someLessThan"
  | "everyGreaterThan"
  | "everyLessThan"
  // Array transformation methods (Phase 3.4)
  | "map"
  | "distinct"
  | "first"
  | "last";

/**
 * Function Call Node
 * Supports custom function invocation for extensibility
 * Example: formatDate(user.createdAt, 'YYYY-MM-DD')
 */
export interface FunctionCallNode {
  type: "functionCall";
  functionName: string;
  arguments: ASTNode[];
  metadata?: NodeMetadata;
}

/**
 * Member Access Node
 * Represents property access on objects with better static analysis
 * Example: user.name, user.address.city
 */
export interface MemberAccessNode {
  type: "memberAccess";
  object: ASTNode;
  property: string;
  metadata?: NodeMetadata;
}
