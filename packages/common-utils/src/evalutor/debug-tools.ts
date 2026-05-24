/**
 * Expression Debugging Tools
 * Provides visualization and debugging utilities for expressions.
 */

import { dslParse } from "./dsl/index.js";
import { expressionEvaluator } from "./expression-evaluator.js";
import type { Expression } from "./dsl/types.js";
import type { EvaluationContext } from "@wf-agent/types";

export interface TraceNode {
  type: string;
  description: string;
  result?: unknown;
  children?: TraceNode[];
  executionTime?: number;
}

export interface EvaluationTrace {
  expression: string;
  root: TraceNode;
  totalTime: number;
  result: unknown;
}

export function visualizeAST(node: Expression, indent: number = 0): string {
  const prefix = "  ".repeat(indent);
  let result = "";

  switch (node.type) {
    case "literal":
      result = `${prefix}Literal: ${JSON.stringify(node.value)} (${node.valueType})`;
      break;

    case "identifier":
      result = `${prefix}Identifier: ${node.name}`;
      break;

    case "memberAccess":
      result = `${prefix}MemberAccess:\n`;
      result += visualizeAST(node.object, indent + 1) + "\n";
      result += `${"  ".repeat(indent + 1)}.${node.property}`;
      break;

    case "unaryMinus":
      result = `${prefix}UnaryMinus:\n`;
      result += visualizeAST(node.operand, indent + 1);
      break;

    case "binary":
      result = `${prefix}Binary: ${node.operator}\n`;
      result += visualizeAST(node.left, indent + 1) + "\n";
      result += visualizeAST(node.right, indent + 1);
      break;

    case "not":
      result = `${prefix}Not:\n`;
      result += visualizeAST(node.operand, indent + 1);
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

    case "call":
      result = `${prefix}Call: ${node.callee.type === "identifier" ? node.callee.name : "method"}(\n`;
      node.arguments.forEach((arg, i) => {
        result += visualizeAST(arg, indent + 1);
        if (i < node.arguments.length - 1) result += ",";
        result += "\n";
      });
      result += `${prefix})`;
      break;

    case "arrayLiteral":
      result = `${prefix}ArrayLiteral: [\n`;
      node.elements.forEach((el, i) => {
        result += visualizeAST(el, indent + 1);
        if (i < node.elements.length - 1) result += ",";
        result += "\n";
      });
      result += `${prefix}]`;
      break;

    default:
      result = `${prefix}Unknown node type: ${(node as any).type}`;
  }

  return result;
}

export function traceEvaluation(expression: string, context: EvaluationContext): EvaluationTrace {
  const startTime = performance.now();

  const ast = dslParse(expression);
  if (ast === null) {
    return {
      expression,
      root: { type: "error", description: "Failed to parse expression" },
      totalTime: performance.now() - startTime,
      result: undefined,
    };
  }

  const root = traceNode(ast, context);

  const endTime = performance.now();
  const totalTime = endTime - startTime;

  const result = expressionEvaluator.evaluate(expression, context);

  return {
    expression,
    root,
    totalTime,
    result,
  };
}

function traceNode(node: Expression, context: EvaluationContext): TraceNode {
  const nodeStart = performance.now();
  let result: TraceNode;

  switch (node.type) {
    case "literal":
      result = {
        type: "literal",
        description: `Literal: ${JSON.stringify(node.value)} (${node.valueType})`,
        result: node.value,
      };
      break;

    case "identifier": {
      const value = getVariableValueFromCtx(node.name, context);
      result = {
        type: "identifier",
        description: `Identifier: ${node.name} => ${JSON.stringify(value)}`,
        result: value,
      };
      break;
    }

    case "memberAccess": {
      const objTrace = traceNode(node.object, context);
      const value =
        objTrace.result != null && typeof objTrace.result === "object"
          ? (objTrace.result as Record<string, unknown>)[node.property]
          : undefined;
      result = {
        type: "memberAccess",
        description: `MemberAccess: .${node.property}`,
        result: value,
        children: [objTrace],
      };
      break;
    }

    case "unaryMinus": {
      const operandTrace = traceNode(node.operand, context);
      const val = typeof operandTrace.result === "number" ? -operandTrace.result : NaN;
      result = {
        type: "unaryMinus",
        description: "Unary minus",
        result: val,
        children: [operandTrace],
      };
      break;
    }

    case "binary": {
      const leftTrace = traceNode(node.left, context);
      const rightTrace = traceNode(node.right, context);
      const op = node.operator;
      let binResult: unknown;

      if (op === "&&" || op === "||") {
        if (op === "&&") binResult = Boolean(leftTrace.result) && Boolean(rightTrace.result);
        else binResult = Boolean(leftTrace.result) || Boolean(rightTrace.result);
      } else if (["+", "-", "*", "/", "%"].includes(op)) {
        const l = typeof leftTrace.result === "number" ? leftTrace.result : NaN;
        const r = typeof rightTrace.result === "number" ? rightTrace.result : NaN;
        switch (op) {
          case "+": binResult = l + r; break;
          case "-": binResult = l - r; break;
          case "*": binResult = l * r; break;
          case "/": binResult = r !== 0 ? l / r : NaN; break;
          case "%": binResult = l % r; break;
        }
      } else {
        binResult = compareValues(leftTrace.result, op, rightTrace.result);
      }

      result = {
        type: "binary",
        description: `Binary: ${op}`,
        result: binResult,
        children: [leftTrace, rightTrace],
      };
      break;
    }

    case "not": {
      const operandTrace = traceNode(node.operand, context);
      result = {
        type: "not",
        description: "Logical NOT",
        result: !operandTrace.result,
        children: [operandTrace],
      };
      break;
    }

    case "ternary": {
      const condTrace = traceNode(node.condition, context);
      let branchTrace: TraceNode;
      if (condTrace.result) {
        branchTrace = traceNode(node.consequent, context);
      } else {
        branchTrace = traceNode(node.alternate, context);
      }
      result = {
        type: "ternary",
        description: "Ternary",
        result: branchTrace.result,
        children: [condTrace, branchTrace],
      };
      break;
    }

    case "call": {
      const calleeTrace = traceNode(node.callee, context);
      const argTraces = node.arguments.map((arg) => traceNode(arg, context));
      const evalResult = expressionEvaluator.evaluateAST(node, context);
      result = {
        type: "call",
        description: `Call: ${node.callee.type === "identifier" ? node.callee.name : "method"}`,
        result: evalResult,
        children: [calleeTrace, ...argTraces],
      };
      break;
    }

    case "arrayLiteral": {
      const elementTraces = node.elements.map((el) => traceNode(el, context));
      const values = elementTraces.map((t) => t.result);
      result = {
        type: "arrayLiteral",
        description: "Array literal",
        result: values,
        children: elementTraces,
      };
      break;
    }

    default:
      const fallback = expressionEvaluator.evaluateAST(node, context);
      result = {
        type: (node as any).type || "unknown",
        description: `${(node as any).type || "unknown"} node`,
        result: fallback,
      };
  }

  const nodeEnd = performance.now();
  result.executionTime = nodeEnd - nodeStart;
  return result;
}

function getVariableValueFromCtx(path: string, context: EvaluationContext): unknown {
  const parts = path.split(".");
  const firstPart = parts[0];
  if (!firstPart) return undefined;
  let current: unknown = (context.variables as Record<string, unknown>)[firstPart];
  for (let i = 1; i < parts.length; i++) {
    const part = parts[i];
    if (!part) return undefined;
    if (current == null || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

function compareValues(left: unknown, operator: string, right: unknown): boolean {
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
    case "contains":
      return String(left).includes(String(right));
    case "in":
      return Array.isArray(right) && right.includes(left);
    default:
      return false;
  }
}

export function formatTrace(trace: EvaluationTrace, indent: number = 0): string {
  const prefix = "  ".repeat(indent);
  let result = `${prefix}Expression: ${trace.expression}\n`;
  result += `${prefix}Result: ${JSON.stringify(trace.result)}\n`;
  result += `${prefix}Total Time: ${trace.totalTime.toFixed(3)}ms\n`;
  result += `${prefix}Trace:\n`;
  result += formatTraceNode(trace.root, indent + 1);
  return result;
}

export interface StructuredTraceMetadata {
  expression: string;
  result: unknown;
  totalTimeMs: number;
  tree: StructuredTraceNode;
}

interface StructuredTraceNode {
  type: string;
  description: string;
  result?: unknown;
  executionTimeMs?: number;
  children?: StructuredTraceNode[];
}

export function formatTraceAsJson(trace: EvaluationTrace): StructuredTraceMetadata {
  return {
    expression: trace.expression,
    result: trace.result,
    totalTimeMs: trace.totalTime,
    tree: formatTraceNodeAsStructured(trace.root),
  };
}

function formatTraceNodeAsStructured(node: TraceNode): StructuredTraceNode {
  const structured: StructuredTraceNode = {
    type: node.type,
    description: node.description,
    result: node.result,
    executionTimeMs: node.executionTime,
  };

  if (node.children && node.children.length > 0) {
    structured.children = node.children.map(formatTraceNodeAsStructured);
  }

  return structured;
}

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
    node.children.forEach((child) => {
      result += formatTraceNode(child, indent + 1);
    });
  }

  return result;
}