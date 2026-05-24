import { conditionLexer } from "./tokens.js";
import { conditionParser } from "./condition-parser.js";
import { conditionCstToAstVisitor } from "./condition-cst-to-ast.js";
import type { Expression, ParseResult, DslError } from "./types.js";
import type { CstNode, IToken } from "chevrotain";

/**
 * Parse expression to CST (Concrete Syntax Tree)
 */
export function parseToCst(expression: string): { cst: CstNode | null; errors: DslError[] } {
  try {
    const lexResult = conditionLexer.tokenize(expression);

    if (lexResult.errors.length > 0) {
      return {
        cst: null,
        errors: lexResult.errors.map(err => ({
          message: err.message,
          location: {
            startOffset: err.offset,
            endOffset: err.offset + 1,
            line: err.line ?? 1, // Use line from error or default to 1
            column: err.column ?? 1, // Use column from error or default to 1
          },
          severity: "error" as "error",
        })),
      };
    }

    conditionParser.input = lexResult.tokens;
    const cst = conditionParser.expression();

    const parseErrors = conditionParser.errors.map(err => ({
      message: err.message,
      location: {
        startOffset: err.token.startOffset,
        endOffset: err.token.endOffset ?? err.token.startOffset + 1,
        line: err.token.startLine ?? 1,
        column: err.token.startColumn ?? 1,
      },
      severity: "error",
    }));

    return {
      cst,
      errors: parseErrors as DslError[],
    };
  } catch (error) {
    return {
      cst: null,
      errors: [
        {
          message: error instanceof Error ? error.message : "Unknown parsing error",
          location: {
            startOffset: 0,
            endOffset: expression.length,
            line: 1,
            column: 1,
          },
          severity: "error",
        },
      ],
    };
  }
}

/**
 * Convert CST to AST
 */
export function cstToAst(cst: CstNode): Expression {
  return conditionCstToAstVisitor.visit(cst) as Expression;
}

/**
 * Parse expression to AST (full pipeline)
 */
export function dslParse(expression: string): Expression {
  const { cst, errors } = parseToCst(expression);

  if (errors.length > 0) {
    throw new Error(`Parsing failed: ${errors.map(e => e.message).join(", ")}`);
  }

  if (!cst) {
    throw new Error("No CST generated");
  }

  return cstToAst(cst);
}

/**
 * Parse expression with detailed error information
 */
export function dslParseWithErrors(expression: string): ParseResult {
  const { cst, errors } = parseToCst(expression);

  if (errors.length > 0 || !cst) {
    return {
      ast: null,
      errors,
    };
  }

  try {
    const ast = cstToAst(cst);
    return {
      ast,
      errors: [],
    };
  } catch (error) {
    return {
      ast: null,
      errors: [
        {
          message: error instanceof Error ? error.message : "AST conversion error",
          location: {
            startOffset: 0,
            endOffset: expression.length,
            line: 1,
            column: 1,
          },
          severity: "error",
        },
      ],
    };
  }
}

/**
 * Validate expression without full evaluation
 */
export function dslValidate(expression: string): { valid: boolean; errors: DslError[] } {
  const result = dslParseWithErrors(expression);
  return {
    valid: result.ast !== null,
    errors: result.errors,
  };
}

/**
 * Tokenize expression (for debugging)
 */
export function tokenizeExpression(expression: string): IToken[] {
  return conditionLexer.tokenize(expression).tokens;
}

// Export types
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
  DslError,
  ParseResult,
} from "./types.js";

// Export core components
export { conditionLexer } from "./tokens.js";
export { conditionParser } from "./condition-parser.js";
export { conditionCstToAstVisitor } from "./condition-cst-to-ast.js";
