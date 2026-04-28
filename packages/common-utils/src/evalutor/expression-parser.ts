/**
 * ExpressionParser - 表达式解析器
 * 提供表达式解析功能，支持构建抽象语法树（AST）
 *
 * 支持的表达式格式：
 * - 比较操作：user.age == 18, score > 60, name contains 'admin'
 * - 逻辑操作：age >= 18 && age <= 65, status == 'active' || status == 'pending'
 * - NOT 操作：!user.isActive, !(age < 18)
 * - 算术运算：user.age + 1, price * 0.9, count % 2
 * - 字符串方法：user.name.startsWith('J'), user.email.endsWith('@example.com')
 * - 三元运算符：age >= 18 ? 'adult' : 'minor'
 */

import { validateExpression } from "./security-validator.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { ASTNode } from "./ast-types.js";
import {
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

/**
 * Parsing expression strings (backward compatible)
 * @param expression expression string
 * @returns parsing result { variablePath, operator, value }
 * @deprecated Use parseAST instead.
 */
export function parseExpression(
  expression: string,
): { variablePath: string; operator: string; value: unknown } | null {
  // Validating Expression Security
  validateExpression(expression);

  const trimmed = expression.trim();

  // Handling pure boolean expressions (true or false)
  if (trimmed === "true" || trimmed === "false") {
    return {
      variablePath: "",
      operator: "==",
      value: trimmed === "true",
    };
  }

  // Trying to match various operators
  const operators = [
    { pattern: /(.+?)\s*===\s*(.+)/, op: "==" },
    { pattern: /(.+?)\s*!==\s*(.+)/, op: "!=" },
    { pattern: /(.+?)\s*==\s*(.+)/, op: "==" },
    { pattern: /(.+?)\s*!=\s*(.+)/, op: "!=" },
    { pattern: /(.+?)\s*>=\s*(.+)/, op: ">=" },
    { pattern: /(.+?)\s*<=\s*(.+)/, op: "<=" },
    { pattern: /(.+?)\s*>\s*(.+)/, op: ">" },
    { pattern: /(.+?)\s*<\s*(.+)/, op: "<" },
    { pattern: /(.+?)\s+contains\s+(.+)/i, op: "contains" },
    { pattern: /(.+?)\s+in\s+(.+)/i, op: "in" },
  ];

  for (const { pattern, op } of operators) {
    const match = trimmed.match(pattern);
    if (match && match[1] && match[2]) {
      const variablePath = match[1].trim();
      const valueStr = match[2].trim();

      const value = parseValue(valueStr);
      return { variablePath, operator: op, value };
    }
  }

  return null;
}

/**
 * Parsing value strings
 * @param valueStr Value string
 * @returns Parsed value
 */
export function parseValue(valueStr: string): unknown {
  // Array: ['admin', 'user']
  if (valueStr.startsWith("[") && valueStr.endsWith("]")) {
    const arrayContent = valueStr.slice(1, -1).trim();
    if (!arrayContent) {
      return [];
    }
    return arrayContent.split(",").map(item => parseValue(item.trim()));
  }

  // String: 'active' or "active"
  if (
    (valueStr.startsWith("'") && valueStr.endsWith("'")) ||
    (valueStr.startsWith('"') && valueStr.endsWith('"'))
  ) {
    return valueStr.slice(1, -1);
  }

  // Boolean: true or false
  if (valueStr === "true") {
    return true;
  }
  if (valueStr === "false") {
    return false;
  }

  // null
  if (valueStr === "null") {
    return null;
  }

  // figure
  if (/^-?\d+\.?\d*$/.test(valueStr)) {
    return parseFloat(valueStr);
  }

  // Variable references (values that do not begin with quotes and are not keywords/numbers)
  // Returns a special flag indicating that this is a reference to a variable
  return { __isVariableRef: true, path: valueStr };
}

/**
 * Parsing an expression as an AST (Abstract Syntax Tree)
 * @param expression expression string
 * @returns AST node
 */
export function parseAST(expression: string): ASTNode {
  // Validating Expression Security
  validateExpression(expression);

  const trimmed = expression.trim();

  // Handling ternary operators (lowest priority)
  const ternaryIndex = findTernaryOperator(trimmed);
  if (ternaryIndex !== -1) {
    const condition = trimmed.slice(0, ternaryIndex).trim();
    const afterQuestion = trimmed.slice(ternaryIndex + 1).trim();
    const colonIndex = findColonInTernary(afterQuestion);

    if (colonIndex !== -1) {
      const consequent = afterQuestion.slice(0, colonIndex).trim();
      const alternate = afterQuestion.slice(colonIndex + 1).trim();

      return {
        type: "ternary",
        condition: parseAST(condition),
        consequent: parseAST(consequent),
        alternate: parseAST(alternate),
      } as TernaryNode;
    }
  }

  // Handling NOT operations
  if (trimmed.startsWith("!")) {
    const operand = trimmed.slice(1).trim();
    return {
      type: "not",
      operand: parseAST(operand),
    } as NotNode;
  }

  // Handling bracketed expressions
  if (trimmed.startsWith("(") && trimmed.endsWith(")")) {
    // Checking for bracket matches
    let depth = 0;
    for (let i = 0; i < trimmed.length; i++) {
      if (trimmed[i] === "(") depth++;
      if (trimmed[i] === ")") depth--;
      if (depth === 0 && i < trimmed.length - 1) {
        // Parentheses don't match, not a full parenthesis expression
        break;
      }
    }
    if (depth === 0) {
      // Remove the outer parentheses and parse recursively.
      return parseAST(trimmed.slice(1, -1));
    }
  }

  // handle literal quantities
  if (trimmed === "true") {
    return { type: "boolean", value: true } as BooleanLiteralNode;
  }
  if (trimmed === "false") {
    return { type: "boolean", value: false } as BooleanLiteralNode;
  }
  if (trimmed === "null") {
    return { type: "null", value: null } as NullLiteralNode;
  }
  if (/^-?\d+\.?\d*$/.test(trimmed)) {
    return { type: "number", value: parseFloat(trimmed) } as NumberLiteralNode;
  }
  if (
    (trimmed.startsWith("'") && trimmed.endsWith("'")) ||
    (trimmed.startsWith('"') && trimmed.endsWith('"'))
  ) {
    return { type: "string", value: trimmed.slice(1, -1) } as StringLiteralNode;
  }

  // String handling methods
  const stringMethodMatch = trimmed.match(
    /^(.+?)\.(startsWith|endsWith|length|toLowerCase|toUpperCase|trim)(?:\((.*?)\))?$/,
  );
  if (stringMethodMatch && stringMethodMatch[1] && stringMethodMatch[2]) {
    const variablePath = stringMethodMatch[1].trim();
    const method = stringMethodMatch[2] as string;
    const argument = stringMethodMatch[3] ? parseValue(stringMethodMatch[3].trim()) : undefined;

    return {
      type: "stringMethod",
      variablePath,
      method,
      argument,
    } as StringMethodNode;
  }

  // Find the outermost logical operator (|| has lower priority than &&)
  const orIndex = findTopLevelOperator(trimmed, "||");
  if (orIndex !== -1) {
    const left = trimmed.slice(0, orIndex).trim();
    const right = trimmed.slice(orIndex + 2).trim();
    return {
      type: "logical",
      operator: "||",
      left: parseAST(left),
      right: parseAST(right),
    } as LogicalNode;
  }

  const andIndex = findTopLevelOperator(trimmed, "&&");
  if (andIndex !== -1) {
    const left = trimmed.slice(0, andIndex).trim();
    const right = trimmed.slice(andIndex + 2).trim();
    return {
      type: "logical",
      operator: "&&",
      left: parseAST(left),
      right: parseAST(right),
    } as LogicalNode;
  }

  // Parsing comparison expressions (priority over arithmetic operations)
  const parsed = parseExpression(trimmed);
  if (parsed && parsed.variablePath) {
    return {
      type: "comparison",
      variablePath: parsed.variablePath,
      operator: parsed.operator as string,
      value: parsed.value,
    } as ComparisonNode;
  }

  // Find arithmetic operators (priority: * / % > + -)
  const mulDivModIndex = findTopLevelOperator(trimmed, ["*", "/", "%"]);
  if (mulDivModIndex !== -1) {
    const operator = trimmed.substring(mulDivModIndex, mulDivModIndex + 1) as "*" | "/" | "%";
    const left = trimmed.slice(0, mulDivModIndex).trim();
    const right = trimmed.slice(mulDivModIndex + 1).trim();
    return {
      type: "arithmetic",
      operator,
      left: parseAST(left),
      right: parseAST(right),
    } as ArithmeticNode;
  }

  const addSubIndex = findTopLevelOperator(trimmed, ["+", "-"]);
  if (addSubIndex !== -1) {
    const operator = trimmed.substring(addSubIndex, addSubIndex + 1) as "+" | "-";
    const left = trimmed.slice(0, addSubIndex).trim();
    const right = trimmed.slice(addSubIndex + 1).trim();
    return {
      type: "arithmetic",
      operator,
      left: parseAST(left),
      right: parseAST(right),
    } as ArithmeticNode;
  }

  // If neither, it may be a variable reference, treated as a comparison expression
  if (trimmed && !trimmed.includes(" ")) {
    return {
      type: "comparison",
      variablePath: trimmed,
      operator: "==",
      value: true,
    } as ComparisonNode;
  }

  throw new RuntimeValidationError(`Failed to parse expression: "${trimmed}"`, {
    operation: "parse_expression",
    field: "expression",
    value: trimmed,
  });
}

/**
 * Finds the location of the outermost logical operator.
 * @param expression expression string
 * @param operator Operator or array of operators.
 * @returns operator position, -1 if not present
 */
function findTopLevelOperator(expression: string, operator: string | string[]): number {
  let depth = 0;
  const operators = Array.isArray(operator) ? operator : [operator];

  for (let i = 0; i < expression.length; i++) {
    if (expression[i] === "(") depth++;
    if (expression[i] === ")") depth--;

    if (depth === 0) {
      for (const op of operators) {
        if (expression.substring(i, i + op.length) === op) {
          return i;
        }
      }
    }
  }
  return -1;
}

/**
 * Finding the Question Mark Position of a Ternary Operator
 * @param expression expression string
 * @returns Question mark position, -1 if not present
 */
function findTernaryOperator(expression: string): number {
  let depth = 0;
  for (let i = 0; i < expression.length; i++) {
    if (expression[i] === "(") depth++;
    if (expression[i] === ")") depth--;
    if (depth === 0 && expression[i] === "?") {
      return i;
    }
  }
  return -1;
}

/**
 * Finding colon positions after question marks in ternary operators
 * @param expression The expression after the question mark
 * @returns colon position, -1 if not present
 */
function findColonInTernary(expression: string): number {
  let depth = 0;
  for (let i = 0; i < expression.length; i++) {
    if (expression[i] === "(") depth++;
    if (expression[i] === ")") depth--;
    if (depth === 0 && expression[i] === ":") {
      return i;
    }
  }
  return -1;
}
