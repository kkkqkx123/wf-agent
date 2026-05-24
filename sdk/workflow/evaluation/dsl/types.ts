// Source location information
export interface SourceLocation {
  start: number;
  end: number;
  line?: number;
  column?: number;
}

// Node metadata (location, comments, etc.)
export interface NodeMetadata {
  location?: SourceLocation;
  comments?: string[];
  custom?: any;
}

// Unified AST node types (matching the architecture document)
export type Expression =
  | LiteralExpr
  | IdentifierExpr
  | MemberAccessExpr
  | UnaryMinusExpr
  | BinaryExpr
  | NotExpr
  | TernaryExpr
  | CallExpr
  | ArrayLiteralExpr;

// Literal expressions
export interface LiteralExpr {
  type: "literal";
  valueType: "boolean" | "number" | "string" | "null";
  value: any;
  metadata?: NodeMetadata;
}

// Identifier expressions
export interface IdentifierExpr {
  type: "identifier";
  name: string;
  metadata?: NodeMetadata;
}

// Member access expressions
export interface MemberAccessExpr {
  type: "memberAccess";
  object: Expression; // Can be IdentifierExpr or another MemberAccessExpr
  property: string;
  metadata?: NodeMetadata;
}

// Unary minus (NEW - replaces 0-x hack)
export interface UnaryMinusExpr {
  type: "unaryMinus";
  operand: Expression;
  metadata?: NodeMetadata;
}

// Binary operator types
export type BinaryOperator =
  // Comparison
  | "=="
  | "!="
  | ">"
  | "<"
  | ">="
  | "<="
  | "contains"
  | "in"
  // Logical
  | "&&"
  | "||"
  // Arithmetic
  | "+"
  | "-"
  | "*"
  | "/"
  | "%";

// Binary expressions (UNIFIED)
export interface BinaryExpr {
  type: "binary";
  operator: BinaryOperator;
  left: Expression;
  right: Expression;
  metadata?: NodeMetadata;
}

// NOT expression
export interface NotExpr {
  type: "not";
  operand: Expression;
  metadata?: NodeMetadata;
}

// Ternary expression
export interface TernaryExpr {
  type: "ternary";
  condition: Expression;
  consequent: Expression;
  alternate: Expression;
  metadata?: NodeMetadata;
}

// Call expressions (UNIFIED)
export interface CallExpr {
  type: "call";
  // Caller can be identifier (function), memberAccess (method), or any expression
  callee: Expression;
  arguments: Expression[];
  metadata?: NodeMetadata;
  // Optional hint for type checking (doesn't affect structure)
  methodKind?: "arrayMethod" | "stringMethod" | "function";
}

// Array literal expressions
export interface ArrayLiteralExpr {
  type: "arrayLiteral";
  elements: Expression[];
  metadata?: NodeMetadata;
}

// CST node type (simplified) - now compatible with Expression
export interface CstNode {
  type: string;
  name: string;
  children: Record<string, any>;
}

// DSL error type
export interface DslError {
  message: string;
  location: {
    startOffset: number;
    endOffset: number;
    line: number;
    column: number;
  };
  suggestions?: string[];
  severity: "error" | "warning";
}

// Parse result with errors
export interface ParseResult {
  ast: Expression | null;
  errors: DslError[];
}
