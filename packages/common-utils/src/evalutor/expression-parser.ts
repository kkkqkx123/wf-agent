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
  ArrayMethodNode,
  ArrayMethodComparisonNode,
  ArrayMethodName,
} from "./ast-types.js";

/**
 * Array method names pattern for regex
 * Includes all methods from Phase 1, 2, and 3
 */
const ARRAY_METHOD_NAMES = 
  'someEqual|someContains|everyEqual|everyHas|' +
  'countWhere|countWhereContains|findEqual|findContains|' +
  'has|hasContains|' +
  'sum|avg|min|max|' +
  'someGreaterThan|someLessThan|everyGreaterThan|everyLessThan|' +
  'map|distinct|first|last';

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

  // Find the outermost logical operator (|| has lower priority than &&)
  // Check this BEFORE array method comparisons to avoid greedy matching issues
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

  // Array method calls
  const arrayMethodMatch = trimmed.match(
    new RegExp(`^(.+)\\.(${ARRAY_METHOD_NAMES})\\((.*)\\)$`),
  );
  if (arrayMethodMatch && arrayMethodMatch[1] && arrayMethodMatch[2]) {
    const arrayPath = arrayMethodMatch[1].trim();
    const methodName = arrayMethodMatch[2] as ArrayMethodName;
    const argsStr = (arrayMethodMatch[3] || '').trim();
    
    // Methods that don't require arguments: first(), last()
    const noArgMethods = ['first', 'last'];
    if (noArgMethods.includes(methodName)) {
      return {
        type: "arrayMethod",
        method: methodName,
        arrayPath,
        propertyName: '',
        value: undefined,
      } as ArrayMethodNode;
    }
    
    // Parse arguments: propertyName, value?
    const rawArgs = parseFunctionArguments(argsStr);
    
    if (rawArgs.length < 1 || rawArgs.length > 2) {
      throw new RuntimeValidationError(
        `Array method ${methodName} requires 1-2 arguments: (propertyName, value?)`,
        {
          operation: "parse_array_method",
          field: "arguments",
          value: argsStr,
        },
      );
    }
    
    // First argument is property name (remove quotes if present)
    let propertyName = rawArgs[0] || '';
    if ((propertyName.startsWith("'") && propertyName.endsWith("'")) || 
        (propertyName.startsWith('"') && propertyName.endsWith('"'))) {
      propertyName = propertyName.slice(1, -1);
    }
    
    // Second argument (optional) should be parsed as a value
    let value: unknown = undefined;
    if (rawArgs.length === 2 && rawArgs[1] !== undefined) {
      value = parseValue(rawArgs[1]);
    }
    
    return {
      type: "arrayMethod",
      method: methodName,
      arrayPath,
      propertyName,
      value,
    } as ArrayMethodNode;
  }

  // Check for pattern: arrayPath.method(args) OP value
  const arrayMethodComparisonMatch = trimmed.match(
    new RegExp(`^(.+)\\.(${ARRAY_METHOD_NAMES})\\((.+)\\)\\s*(==|!=|>=|<=|>|<)\\s*(.+)$`),
  );
  if (arrayMethodComparisonMatch && arrayMethodComparisonMatch[1] && arrayMethodComparisonMatch[2] && arrayMethodComparisonMatch[3] && arrayMethodComparisonMatch[4] && arrayMethodComparisonMatch[5]) {
    const [, arrayPath, methodName, argsStr, operator, compareValueStr] = arrayMethodComparisonMatch;
    
    // Parse the array method part
    const rawArgs = parseFunctionArguments(argsStr!.trim());
    
    if (rawArgs.length < 1 || rawArgs.length > 2) {
      throw new RuntimeValidationError(
        `Array method ${methodName} requires 1-2 arguments: (propertyName, value?)`,
        {
          operation: "parse_array_method_comparison",
          field: "arguments",
          value: argsStr,
        },
      );
    }
    
    let propertyName = rawArgs[0] || '';
    if ((propertyName.startsWith("'") && propertyName.endsWith("'")) || 
        (propertyName.startsWith('"') && propertyName.endsWith('"'))) {
      propertyName = propertyName.slice(1, -1);
    }
    
    let value: unknown = undefined;
    if (rawArgs.length === 2 && rawArgs[1] !== undefined) {
      value = parseValue(rawArgs[1]);
    }
    
    const methodNode: ArrayMethodNode = {
      type: "arrayMethod",
      method: methodName as ArrayMethodName,
      arrayPath: arrayPath!.trim(),
      propertyName,
      value,
    };
    
    // Parse the comparison value
    const compareValue = parseValue(compareValueStr!.trim());
    
    return {
      type: "arrayMethodComparison",
      methodNode,
      operator: operator as ComparisonNode["operator"],
      compareValue,
    } as ArrayMethodComparisonNode;
  }

  // Parsing comparison expressions (priority over arithmetic operations)
  const comparisonResult = parseComparisonExpression(trimmed);
  if (comparisonResult) {
    return {
      type: "comparison",
      variablePath: comparisonResult.variablePath,
      operator: comparisonResult.operator,
      value: comparisonResult.value,
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

/**
 * Parse comparison expression
 * @param expression expression string
 * @returns parsing result { variablePath, operator, value } or null if not a comparison
 */
function parseComparisonExpression(
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

  // Check if left side is an array method call
  const arrayMethodPattern = new RegExp(`^(.+)\\.(${ARRAY_METHOD_NAMES})\\((.+)\\)$`);
  
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
      let variablePath = match[1].trim();
      const valueStr = match[2].trim();
      
      // If the left side looks like an array method call, wrap it as a special marker
      // This will be handled by the evaluator
      if (arrayMethodPattern.test(variablePath)) {
        // Return with a flag indicating this is an array method expression
        const value = parseValue(valueStr);
        return { 
          variablePath: `__ARRAY_METHOD__:${variablePath}`, 
          operator: op, 
          value 
        };
      }

      const value = parseValue(valueStr);
      return { variablePath, operator: op, value };
    }
  }

  return null;
}

/**
 * Parse function arguments (comma-separated, respecting quotes and parentheses)
 * @param argsStr Arguments string
 * @returns Array of parsed argument strings
 */
function parseFunctionArguments(argsStr: string): string[] {
  const args: string[] = [];
  let currentArg = '';
  let depth = 0;
  let inSingleQuote = false;
  let inDoubleQuote = false;
  
  for (let i = 0; i < argsStr.length; i++) {
    const char = argsStr[i];
    
    // Handle quotes
    if (char === "'" && !inDoubleQuote) {
      inSingleQuote = !inSingleQuote;
      currentArg += char;
    } else if (char === '"' && !inSingleQuote) {
      inDoubleQuote = !inDoubleQuote;
      currentArg += char;
    } else if ((char === '(' || char === '[') && !inSingleQuote && !inDoubleQuote) {
      depth++;
      currentArg += char;
    } else if ((char === ')' || char === ']') && !inSingleQuote && !inDoubleQuote) {
      depth--;
      currentArg += char;
    } else if (char === ',' && depth === 0 && !inSingleQuote && !inDoubleQuote) {
      // Comma at top level separates arguments
      args.push(currentArg.trim());
      currentArg = '';
    } else {
      currentArg += char;
    }
  }
  
  // Add the last argument
  if (currentArg.trim()) {
    args.push(currentArg.trim());
  }
  
  return args;
}
