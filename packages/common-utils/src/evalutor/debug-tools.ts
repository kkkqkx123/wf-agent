/**
 * Expression Debugging Tools
 * Provides visualization and debugging utilities for expressions (Phase 2)
 */

import type { ASTNode } from "./ast-types.js";
import { parseAST } from "./expression-parser.js";
import { expressionEvaluator } from "./expression-evaluator.js";
import type { EvaluationContext } from "@wf-agent/types";

/**
 * AST node with evaluation trace information
 */
export interface TraceNode {
  /** Node type */
  type: string;
  /** Node description */
  description: string;
  /** Evaluation result */
  result?: unknown;
  /** Child nodes */
  children?: TraceNode[];
  /** Execution time in milliseconds */
  executionTime?: number;
}

/**
 * Full evaluation trace
 */
export interface EvaluationTrace {
  /** Original expression */
  expression: string;
  /** Root trace node */
  root: TraceNode;
  /** Total execution time */
  totalTime: number;
  /** Final result */
  result: unknown;
}

/**
 * Visualize AST as a tree structure (text-based)
 * @param node AST node
 * @param indent Current indentation level
 * @returns Tree representation string
 */
export function visualizeAST(node: ASTNode, indent: number = 0): string {
  const prefix = "  ".repeat(indent);
  let result = "";

  switch (node.type) {
    case "boolean":
      result = `${prefix}Boolean: ${node.value}`;
      break;
    case "number":
      result = `${prefix}Number: ${node.value}`;
      break;
    case "string":
      result = `${prefix}String: "${node.value}"`;
      break;
    case "null":
      result = `${prefix}Null`;
      break;
    case "comparison":
      result = `${prefix}Comparison: ${node.variablePath} ${node.operator} ${JSON.stringify(node.value)}`;
      break;
    case "logical":
      result = `${prefix}Logical: ${node.operator}\n`;
      result += visualizeAST(node.left, indent + 1) + "\n";
      result += visualizeAST(node.right, indent + 1);
      break;
    case "not":
      result = `${prefix}Not:\n`;
      result += visualizeAST(node.operand, indent + 1);
      break;
    case "arithmetic":
      result = `${prefix}Arithmetic: ${node.operator}\n`;
      result += visualizeAST(node.left, indent + 1) + "\n";
      result += visualizeAST(node.right, indent + 1);
      break;
    case "stringMethod":
      result = `${prefix}StringMethod: ${node.variablePath}.${node.method}(${node.argument !== undefined ? JSON.stringify(node.argument) : ""})`;
      break;
    case "ternary":
      result = `${prefix}Ternary:\n`;
      result += `${"  ".repeat(indent + 1)}Condition:\n`;
      result += visualizeAST(node.condition, indent + 2) + "\n";
      result += `${"  ".repeat(indent + 1)}Consequent:\n`;
      result += visualizeAST(node.consequent, indent + 2) + "\n";
      result += `${"  ".repeat(indent + 1)}Alternate:\n`;
      result += visualizeAST(node.alternate, indent + 2);
      break;
    case "arrayMethod":
      result = `${prefix}ArrayMethod: ${node.arrayPath}.${node.method}('${node.propertyName}'${node.value !== undefined ? `, ${JSON.stringify(node.value)}` : ""})`;
      break;
    case "arrayMethodComparison":
      result = `${prefix}ArrayMethodComparison:\n`;
      result += visualizeAST(node.methodNode, indent + 1) + "\n";
      result += `${"  ".repeat(indent + 1)}${node.operator} ${JSON.stringify(node.compareValue)}`;
      break;
    case "functionCall":
      result = `${prefix}FunctionCall: ${node.functionName}(\n`;
      node.arguments.forEach((arg, i) => {
        result += visualizeAST(arg, indent + 1);
        if (i < node.arguments.length - 1) result += ",";
        result += "\n";
      });
      result += `${prefix})`;
      break;
    case "memberAccess":
      result = `${prefix}MemberAccess:\n`;
      result += visualizeAST(node.object, indent + 1) + "\n";
      result += `${"  ".repeat(indent + 1)}.${node.property}`;
      break;
    default:
      result = `${prefix}Unknown node type`;
  }

  return result;
}

/**
 * Trace expression evaluation step by step
 * @param expression Expression to evaluate
 * @param context Evaluation context
 * @returns Detailed evaluation trace
 */
export function traceEvaluation(expression: string, context: EvaluationContext): EvaluationTrace {
  const startTime = performance.now();
  
  // Parse the expression
  const ast = parseAST(expression);
  
  // Build trace tree
  const root = traceNode(ast, context);
  
  const endTime = performance.now();
  const totalTime = endTime - startTime;
  
  // Evaluate to get final result
  const result = expressionEvaluator.evaluate(expression, context);
  
  return {
    expression,
    root,
    totalTime,
    result,
  };
}

/**
 * Trace a single AST node evaluation
 */
function traceNode(node: ASTNode, context: EvaluationContext): TraceNode {
  const nodeStart = performance.now();
  
  let result: TraceNode;
  
  switch (node.type) {
    case "boolean":
      result = {
        type: "boolean",
        description: `Boolean literal: ${node.value}`,
        result: node.value,
      };
      break;
      
    case "number":
      result = {
        type: "number",
        description: `Number literal: ${node.value}`,
        result: node.value,
      };
      break;
      
    case "string":
      result = {
        type: "string",
        description: `String literal: "${node.value}"`,
        result: node.value,
      };
      break;
      
    case "null":
      result = {
        type: "null",
        description: "Null literal",
        result: null,
      };
      break;
      
    case "comparison":
      const varValue = getVariableValue(node.variablePath, context);
      result = {
        type: "comparison",
        description: `Compare: ${node.variablePath} ${node.operator} ${JSON.stringify(node.value)}`,
        result: performComparison(varValue, node.operator, node.value),
        children: [
          {
            type: "variable",
            description: `${node.variablePath} = ${JSON.stringify(varValue)}`,
            result: varValue,
          },
        ],
      };
      break;
      
    case "logical":
      const leftTrace = traceNode(node.left, context);
      const rightTrace = traceNode(node.right, context);
      
      let logicalResult: boolean;
      if (node.operator === "&&") {
        logicalResult = (leftTrace.result as boolean) && (rightTrace.result as boolean);
      } else {
        logicalResult = (leftTrace.result as boolean) || (rightTrace.result as boolean);
      }
      
      result = {
        type: "logical",
        description: `Logical ${node.operator}`,
        result: logicalResult,
        children: [leftTrace, rightTrace],
      };
      break;
      
    case "not":
      const operandTrace = traceNode(node.operand, context);
      result = {
        type: "not",
        description: "Logical NOT",
        result: !(operandTrace.result as boolean),
        children: [operandTrace],
      };
      break;
      
    case "arithmetic":
      const leftArith = traceNode(node.left, context);
      const rightArith = traceNode(node.right, context);
      
      let arithResult: number;
      const leftVal = leftArith.result as number;
      const rightVal = rightArith.result as number;
      
      switch (node.operator) {
        case "+":
          arithResult = leftVal + rightVal;
          break;
        case "-":
          arithResult = leftVal - rightVal;
          break;
        case "*":
          arithResult = leftVal * rightVal;
          break;
        case "/":
          arithResult = leftVal / rightVal;
          break;
        case "%":
          arithResult = leftVal % rightVal;
          break;
        default:
          arithResult = 0;
      }
      
      result = {
        type: "arithmetic",
        description: `Arithmetic: ${node.operator}`,
        result: arithResult,
        children: [leftArith, rightArith],
      };
      break;
      
    default:
      // For other node types, just evaluate without detailed tracing
      const evalResult = expressionEvaluator.evaluateAST(node, context);
      result = {
        type: node.type,
        description: `${node.type} node`,
        result: evalResult,
      };
  }
  
  const nodeEnd = performance.now();
  result.executionTime = nodeEnd - nodeStart;
  
  return result;
}

/**
 * Get variable value from context
 */
function getVariableValue(path: string, context: EvaluationContext): unknown {
  const parts = path.split(".");
  const firstPart = parts[0];
  if (!firstPart) return undefined;
  
  let current: unknown = (context.variables as Record<string, unknown>)[firstPart];
  
  for (let i = 1; i < parts.length; i++) {
    if (current == null || typeof current !== "object") {
      return undefined;
    }
    const part = parts[i];
    if (!part) return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  
  return current;
}

/**
 * Perform comparison operation
 */
function performComparison(left: unknown, operator: string, right: unknown): boolean {
  switch (operator) {
    case "==":
      return left === right;
    case "!=":
      return left !== right;
    case ">":
      return typeof left === "number" && typeof right === "number" && left > right;
    case "<":
      return typeof left === "number" && typeof right === "number" && left < right;
    case ">=":
      return typeof left === "number" && typeof right === "number" && left >= right;
    case "<=":
      return typeof left === "number" && typeof right === "number" && left <= right;
    default:
      return false;
  }
}

/**
 * Format trace as readable text
 * @param trace Evaluation trace
 * @param indent Current indentation
 * @returns Formatted trace string
 */
export function formatTrace(trace: EvaluationTrace, indent: number = 0): string {
  const prefix = "  ".repeat(indent);
  let result = `${prefix}Expression: ${trace.expression}\n`;
  result += `${prefix}Result: ${JSON.stringify(trace.result)}\n`;
  result += `${prefix}Total Time: ${trace.totalTime.toFixed(3)}ms\n`;
  result += `${prefix}Trace:\n`;
  result += formatTraceNode(trace.root, indent + 1);
  
  return result;
}

/**
 * Format a single trace node
 */
function formatTraceNode(node: TraceNode, indent: number = 0): string {
  const prefix = "  ".repeat(indent);
  let result = `${prefix}[${node.type}] ${node.description}`;
  
  if (node.result !== undefined) {
    result += ` => ${JSON.stringify(node.result)}`;
  }
  
  if (node.executionTime !== undefined) {
    result += ` (${node.executionTime.toFixed(3)}ms)`;
  }
  
  result += "\n";
  
  if (node.children) {
    node.children.forEach(child => {
      result += formatTraceNode(child, indent + 1);
    });
  }
  
  return result;
}
