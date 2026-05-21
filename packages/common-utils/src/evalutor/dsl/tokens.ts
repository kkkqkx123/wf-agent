import { createToken, Lexer } from "chevrotain";

// First define Identifier (but we need to handle circular reference)
// We'll create a placeholder and update later
const IdentifierPlaceholder = createToken({
  name: "Identifier",
  pattern: /[a-zA-Z_]\w*/,
});

// Keywords (must be defined before Identifier for priority)
export const True = createToken({
  name: "True",
  pattern: /true/,
  longer_alt: IdentifierPlaceholder,
});

export const False = createToken({
  name: "False",
  pattern: /false/,
  longer_alt: IdentifierPlaceholder,
});

export const Null = createToken({
  name: "Null",
  pattern: /null/,
  longer_alt: IdentifierPlaceholder,
});

export const Contains = createToken({
  name: "Contains",
  pattern: /contains/,
  longer_alt: IdentifierPlaceholder,
});

export const In = createToken({
  name: "In",
  pattern: /in/,
  longer_alt: IdentifierPlaceholder,
});

// Array method names
export const ArrayMethod = createToken({
  name: "ArrayMethod",
  pattern:
    /\b(someEqual|someContains|everyEqual|everyHas|countWhere|countWhereContains|findEqual|findContains|has|hasContains|sum|avg|min|max|someGreaterThan|someLessThan|everyGreaterThan|everyLessThan|map|distinct|first|last)\b/,
});

// String method names
export const StringMethod = createToken({
  name: "StringMethod",
  pattern: /\b(startsWith|endsWith|toLowerCase|toUpperCase|trim)\b/,
});

// Operators
export const ComparisonOp = createToken({ name: "ComparisonOp", pattern: /==|!=|>=|<=|>|</ });
export const LogicalOr = createToken({ name: "LogicalOr", pattern: /\|\|/ });
export const LogicalAnd = createToken({ name: "LogicalAnd", pattern: /&&/ });
export const Not = createToken({ name: "Not", pattern: /!/ });
export const Plus = createToken({ name: "Plus", pattern: /\+/ });
export const Minus = createToken({ name: "Minus", pattern: /-/ });
export const Multiply = createToken({ name: "Multiply", pattern: /\*/ });
export const Divide = createToken({ name: "Divide", pattern: /\// });
export const Modulo = createToken({ name: "Modulo", pattern: /%/ });
export const Ternary = createToken({ name: "Ternary", pattern: /\?/ });
export const Colon = createToken({ name: "Colon", pattern: /:/ });
export const Dot = createToken({ name: "Dot", pattern: /\./ });
export const LParen = createToken({ name: "LParen", pattern: /\(/ });
export const RParen = createToken({ name: "RParen", pattern: /\)/ });
export const LBracket = createToken({ name: "LBracket", pattern: /\[/ });
export const RBracket = createToken({ name: "RBracket", pattern: /\]/ });
export const Comma = createToken({ name: "Comma", pattern: /,/ });

// Literals
export const StringLiteral = createToken({
  name: "StringLiteral",
  pattern: /'([^'\\]|\\.)*'|"([^"\\]|\\.)*"/,
});

export const NumberLiteral = createToken({
  name: "NumberLiteral",
  pattern: /\d+(\.\d+)?/,
});

// Identifier (last - lowest priority) - update the placeholder
export const Identifier = IdentifierPlaceholder;

// Whitespace and comments (skipped)
export const WhiteSpace = createToken({
  name: "WhiteSpace",
  pattern: /\s+/,
  group: Lexer.SKIPPED,
});

export const LineComment = createToken({
  name: "LineComment",
  pattern: /\/\/[^\n\r]*/,
  group: Lexer.SKIPPED,
});

export const BlockComment = createToken({
  name: "BlockComment",
  pattern: /\/\*[\s\S]*?\*\//,
  group: Lexer.SKIPPED,
});

// All tokens in priority order (higher priority first)
export const allTokens = [
  WhiteSpace,
  LineComment,
  BlockComment,
  True,
  False,
  Null,
  Contains,
  In,
  ArrayMethod,
  StringMethod,
  ComparisonOp,
  LogicalOr,
  LogicalAnd,
  Not,
  Plus,
  Minus,
  Multiply,
  Divide,
  Modulo,
  Ternary,
  Colon,
  Dot,
  LParen,
  RParen,
  LBracket,
  RBracket,
  Comma,
  StringLiteral,
  NumberLiteral,
  Identifier,
];

// Create lexer instance
export const conditionLexer = new Lexer(allTokens, {
  ensureOptimizations: true,
});
