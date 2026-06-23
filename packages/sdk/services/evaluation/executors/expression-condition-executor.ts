/**
 * ExpressionConditionExecutor
 * Executes compiled expression AST
 * Implements IExecutor interface
 */

import type { EvaluationContext } from "@wf-agent/types";
import { ExpressionSecurityError, RuntimeValidationError } from "@wf-agent/types";
import {
  validateArrayIndex,
  validateValueType,
  SECURITY_CONFIG,
} from "../shared/security-validator.js";
import { getGlobalLogger } from "@wf-agent/common-utils";
import type {
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
} from "../dsl/types.js";
import { BaseExecutor } from "../base-executor.js";
import type { CompiledUnit, IExecutor } from "../types/index.js";

export class ExpressionConditionExecutor extends BaseExecutor implements IExecutor {
  override readonly logger = getGlobalLogger().child("ExpressionConditionExecutor", { pkg: "sdk/workflow" });
  private registeredFunctions = new Map<string, (...args: unknown[]) => unknown>();
  private arrayMethodCache = new Map<string, { value: unknown; timestamp: number }>();
  private readonly ARRAY_METHOD_CACHE_TTL = 50;

  execute(compiled: CompiledUnit, context: EvaluationContext): unknown {
    this.validateContext(context);

    if (!compiled.ast) {
      throw new Error("Compiled unit missing AST");
    }

    this.validateMemberAccessDepth(compiled.ast as Expression);

    const result = this.evaluateAST(compiled.ast as Expression, context);
    validateValueType(result);
    return result;
  }

  private validateMemberAccessDepth(node: Expression, depth: number = 0): void {
    if (node.type === "memberAccess") {
      const newDepth = depth + 1;
      if (newDepth >= SECURITY_CONFIG.MAX_PATH_DEPTH) {
        throw new RuntimeValidationError(
          `Path depth exceeds maximum limit of ${SECURITY_CONFIG.MAX_PATH_DEPTH}`,
          { operation: "validateMemberAccessDepth", field: "depth", value: newDepth },
        );
      }
      this.validateMemberAccessDepth((node as MemberAccessExpr).object, newDepth);
    } else if (node.type === "binary") {
      const binNode = node as BinaryExpr;
      this.validateMemberAccessDepth(binNode.left, depth);
      this.validateMemberAccessDepth(binNode.right, depth);
    } else if (node.type === "not") {
      this.validateMemberAccessDepth((node as NotExpr).operand, depth);
    } else if (node.type === "unaryMinus") {
      this.validateMemberAccessDepth((node as UnaryMinusExpr).operand, depth);
    } else if (node.type === "ternary") {
      const ternNode = node as TernaryExpr;
      this.validateMemberAccessDepth(ternNode.condition, depth);
      this.validateMemberAccessDepth(ternNode.consequent, depth);
      this.validateMemberAccessDepth(ternNode.alternate, depth);
    } else if (node.type === "call") {
      const callNode = node as CallExpr;
      this.validateMemberAccessDepth(callNode.callee, depth);
      for (const arg of callNode.arguments) {
        this.validateMemberAccessDepth(arg, depth);
      }
    } else if (node.type === "arrayLiteral") {
      for (const el of (node as ArrayLiteralExpr).elements) {
        this.validateMemberAccessDepth(el, depth);
      }
    }
  }

  private evaluateAST(node: Expression, context: EvaluationContext): unknown {
    switch (node.type) {
      case "literal":
        return this.evaluateLiteral(node as LiteralExpr);

      case "identifier":
        return this.evaluateIdentifier(node as IdentifierExpr, context);

      case "memberAccess":
        return this.evaluateMemberAccess(node as MemberAccessExpr, context);

      case "unaryMinus":
        return this.evaluateUnaryMinus(node as UnaryMinusExpr, context);

      case "binary":
        return this.evaluateBinary(node as BinaryExpr, context);

      case "not":
        return this.evaluateNot(node as NotExpr, context);

      case "ternary":
        return this.evaluateTernary(node as TernaryExpr, context);

      case "call":
        return this.evaluateCall(node as CallExpr, context);

      case "arrayLiteral":
        return this.evaluateArrayLiteral(node as ArrayLiteralExpr, context);

      default:
        throw new RuntimeValidationError(
          `Unknown AST node type: ${(node as { type?: string }).type}`,
          { operation: "ast_evaluation", field: "node", value: node },
        );
    }
  }

  private evaluateLiteral(node: LiteralExpr): unknown {
    return node.value;
  }

  private evaluateIdentifier(node: IdentifierExpr, context: EvaluationContext): unknown {
    return this.getVariableValue(node.name, context);
  }

  private evaluateUnaryMinus(node: UnaryMinusExpr, context: EvaluationContext): number {
    const operand = this.evaluateAST(node.operand, context);
    if (typeof operand !== "number") {
      this.logger.warn(`Unary minus on non-number: ${typeof operand}`, { operand });
      return NaN;
    }
    return -operand;
  }

  private evaluateBinary(node: BinaryExpr, context: EvaluationContext): unknown {
    const { operator } = node;

    if (operator === "&&" || operator === "||") {
      return this.evaluateLogical(node, context);
    }

    if (["+", "-", "*", "/", "%"].includes(operator)) {
      return this.evaluateArithmetic(node, context);
    }

    return this.evaluateComparison(node, context);
  }

  private evaluateLogical(node: BinaryExpr, context: EvaluationContext): boolean {
    const leftResult = this.evaluateAST(node.left, context);

    if (node.operator === "&&" && !leftResult) {
      return false;
    }
    if (node.operator === "||" && leftResult) {
      return true;
    }

    const rightResult = this.evaluateAST(node.right, context);

    if (node.operator === "&&") {
      return Boolean(leftResult && rightResult);
    }
    return Boolean(leftResult || rightResult);
  }

  private evaluateNot(node: NotExpr, context: EvaluationContext): boolean {
    const operand = this.evaluateAST(node.operand, context);
    return !operand;
  }

  private evaluateArithmetic(node: BinaryExpr, context: EvaluationContext): number {
    const leftValue = this.evaluateAST(node.left, context);
    const rightValue = this.evaluateAST(node.right, context);

    if (typeof leftValue !== "number" || typeof rightValue !== "number") {
      this.logger.warn(
        `Type mismatch in arithmetic: ${typeof leftValue} ${node.operator} ${typeof rightValue}`,
        { leftValue, rightValue, operator: node.operator },
      );
      return NaN;
    }

    switch (node.operator) {
      case "+":
        return leftValue + rightValue;
      case "-":
        return leftValue - rightValue;
      case "*":
        return leftValue * rightValue;
      case "/":
        if (rightValue === 0) {
          this.logger.warn("Division by zero", { leftValue, rightValue });
          return NaN;
        }
        return leftValue / rightValue;
      case "%":
        return leftValue % rightValue;
      default:
        throw new RuntimeValidationError(`Unknown arithmetic operator: ${node.operator}`, {
          operation: "arithmetic_evaluation",
          field: "operator",
          value: node.operator,
        });
    }
  }

  private evaluateComparison(node: BinaryExpr, context: EvaluationContext): unknown {
    const leftValue = this.evaluateAST(node.left, context);
    const rightValue = this.evaluateAST(node.right, context);

    const { operator } = node;

    if (leftValue === undefined) {
      this.logger.warn(`Left operand evaluated to undefined`, { operator, rightValue });
    }

    switch (operator) {
      case "==":
        return leftValue === rightValue;
      case "!=":
        return leftValue !== rightValue;
      case ">":
        if (typeof leftValue !== "number" || typeof rightValue !== "number") {
          this.logger.warn(`Type mismatch in > comparison`, { leftValue, rightValue });
          return false;
        }
        return leftValue > rightValue;
      case "<":
        if (typeof leftValue !== "number" || typeof rightValue !== "number") {
          this.logger.warn(`Type mismatch in < comparison`, { leftValue, rightValue });
          return false;
        }
        return leftValue < rightValue;
      case ">=":
        if (typeof leftValue !== "number" || typeof rightValue !== "number") {
          this.logger.warn(`Type mismatch in >= comparison`, { leftValue, rightValue });
          return false;
        }
        return leftValue >= rightValue;
      case "<=":
        if (typeof leftValue !== "number" || typeof rightValue !== "number") {
          this.logger.warn(`Type mismatch in <= comparison`, { leftValue, rightValue });
          return false;
        }
        return leftValue <= rightValue;
      case "contains":
        return String(leftValue).includes(String(rightValue));
      case "in":
        if (!Array.isArray(rightValue)) {
          this.logger.warn(`Right operand of 'in' must be an array`, { rightValue });
          return false;
        }
        return rightValue.includes(leftValue);
      default:
        throw new RuntimeValidationError(`Unknown comparison operator: ${operator}`, {
          operation: "comparison_evaluation",
          field: "operator",
          value: operator,
        });
    }
  }

  private evaluateTernary(node: TernaryExpr, context: EvaluationContext): unknown {
    const condition = this.evaluateAST(node.condition, context);
    if (condition) {
      return this.evaluateAST(node.consequent, context);
    }
    return this.evaluateAST(node.alternate, context);
  }

  private evaluateCall(node: CallExpr, context: EvaluationContext): unknown {
    if (node.callee.type === "memberAccess") {
      const methodCallee = node.callee as MemberAccessExpr;
      const methodName = methodCallee.property;
      let methodKind = node.methodKind;

      if (!methodKind) {
        methodKind = this.determineMethodKind(methodName);
      }

      if (methodKind === "arrayMethod" || methodKind === "stringMethod") {
        const obj = this.evaluateAST(methodCallee.object, context);

        if (methodKind === "stringMethod") {
          return this.evaluateStringMethod(methodName, obj, node, context);
        }
        return this.evaluateArrayMethod(methodName, obj, node, context);
      }
    }

    if (node.callee.type === "identifier") {
      const funcName = (node.callee as IdentifierExpr).name;
      const func = this.registeredFunctions.get(funcName);
      if (!func) {
        throw new RuntimeValidationError(`Unknown function: ${funcName}`, {
          operation: "function_call_evaluation",
          field: "functionName",
          value: funcName,
          context: { availableFunctions: Array.from(this.registeredFunctions.keys()) },
        });
      }
      try {
        const args = node.arguments.map(arg => this.evaluateAST(arg, context));
        return func(...args);
      } catch (error) {
        this.logger.error(`Function execution failed: ${funcName}`, {
          functionName: funcName,
          error: error instanceof Error ? error.message : String(error),
        });
        throw new RuntimeValidationError(
          `Function execution failed: ${funcName} - ${error instanceof Error ? error.message : String(error)}`,
          { operation: "function_call_execution", field: "functionName", value: funcName },
        );
      }
    }

    throw new RuntimeValidationError("Unknown call expression type", {
      operation: "call_evaluation",
      field: "callee",
      value: node.callee,
    });
  }

  private determineMethodKind(methodName: string): "arrayMethod" | "stringMethod" | "function" {
    const arrayMethods = [
      "someEqual",
      "someContains",
      "everyEqual",
      "everyHas",
      "countWhere",
      "countWhereContains",
      "findEqual",
      "findContains",
      "has",
      "hasContains",
      "sum",
      "avg",
      "min",
      "max",
      "someGreaterThan",
      "someLessThan",
      "everyGreaterThan",
      "everyLessThan",
      "map",
      "distinct",
      "first",
      "last",
    ];

    if (arrayMethods.includes(methodName)) {
      return "arrayMethod";
    }

    const stringMethods = ["startsWith", "endsWith", "toLowerCase", "toUpperCase", "trim"];
    if (stringMethods.includes(methodName)) {
      return "stringMethod";
    }

    return "function";
  }

  private evaluateStringMethod(
    methodName: string,
    stringValue: unknown,
    node: CallExpr,
    context: EvaluationContext,
  ): unknown {
    if (typeof stringValue !== "string") {
      this.logger.warn(`String method '${methodName}' called on non-string`, {
        method: methodName,
        valueType: typeof stringValue,
      });
      return false;
    }

    const args = node.arguments.map(arg => this.evaluateAST(arg, context));

    switch (methodName) {
      case "startsWith":
        return stringValue.startsWith(String(args[0] || ""));
      case "endsWith":
        return stringValue.endsWith(String(args[0] || ""));
      case "length":
        return stringValue.length;
      case "toLowerCase":
        return stringValue.toLowerCase();
      case "toUpperCase":
        return stringValue.toUpperCase();
      case "trim":
        return stringValue.trim();
      default:
        throw new RuntimeValidationError(`Unknown string method: ${methodName}`, {
          operation: "string_method_evaluation",
          field: "method",
          value: methodName,
        });
    }
  }

  private evaluateArrayMethod(
    methodName: string,
    array: unknown,
    node: CallExpr,
    context: EvaluationContext,
  ): unknown {
    const cacheKey = `${methodName}:${JSON.stringify(node.arguments.map(a => this.evaluateAST(a, context)))}`;
    const cached = this.arrayMethodCache.get(cacheKey);
    const now = Date.now();
    if (cached && now - cached.timestamp < this.ARRAY_METHOD_CACHE_TTL) {
      return cached.value;
    }

    const result = this.computeArrayMethod(methodName, array, node, context);

    this.arrayMethodCache.set(cacheKey, { value: result, timestamp: now });
    if (this.arrayMethodCache.size > 100) {
      this.cleanArrayMethodCache(now);
    }
    return result;
  }

  private computeArrayMethod(
    methodName: string,
    array: unknown,
    node: CallExpr,
    context: EvaluationContext,
  ): unknown {
    if (array === undefined || array === null) {
      this.logger.warn(`Array resolved to ${array === undefined ? "undefined" : "null"}`, {
        method: methodName,
      });
      return this.handleEmptyArray(methodName);
    }

    if (!Array.isArray(array)) {
      this.logger.warn(`Expected array for method '${methodName}', got ${typeof array}`, {
        method: methodName,
        actualType: typeof array,
      });
      return false;
    }

    const args = node.arguments.map(arg => this.evaluateAST(arg, context));
    const propertyName = args.length > 0 ? String(args[0]) : "";
    const value = args.length > 1 ? args[1] : undefined;

    if (array.length === 0) {
      return this.handleEmptyArray(methodName);
    }

    switch (methodName) {
      case "someEqual":
        return array.some(item => this.getPropertyValue(item, propertyName) === value);
      case "someContains":
        return array.some(item => {
          const propValue = this.getPropertyValue(item, propertyName);
          return typeof propValue === "string" && propValue.includes(String(value));
        });
      case "everyEqual":
        return array.every(item => this.getPropertyValue(item, propertyName) === value);
      case "everyHas":
        return array.every(item => propertyName in item && item[propertyName] != null);
      case "countWhere":
        return array.filter(item => this.getPropertyValue(item, propertyName) === value).length;
      case "countWhereContains":
        return array.filter(item => {
          const propValue = this.getPropertyValue(item, propertyName);
          return typeof propValue === "string" && propValue.includes(String(value));
        }).length;
      case "findEqual":
        return array.find(item => this.getPropertyValue(item, propertyName) === value) ?? null;
      case "findContains":
        return (
          array.find(item => {
            const propValue = this.getPropertyValue(item, propertyName);
            return typeof propValue === "string" && propValue.includes(String(value));
          }) ?? null
        );
      case "has":
        return array.some(item => this.getPropertyValue(item, propertyName) === value);
      case "hasContains":
        return array.some(item => {
          const propValue = this.getPropertyValue(item, propertyName);
          return typeof propValue === "string" && propValue.includes(String(value));
        });
      case "sum":
        return array.reduce((acc, item) => {
          const val = this.getPropertyValue(item, propertyName);
          return typeof val === "number" ? acc + val : acc;
        }, 0);
      case "avg": {
        const numericValues = array
          .map(item => this.getPropertyValue(item, propertyName))
          .filter(v => typeof v === "number");
        if (numericValues.length === 0) return 0;
        return numericValues.reduce((acc, val) => acc + val, 0) / numericValues.length;
      }
      case "min": {
        const values = array
          .map(item => this.getPropertyValue(item, propertyName))
          .filter(v => typeof v === "number");
        return values.length > 0 ? Math.min(...values) : null;
      }
      case "max": {
        const maxValues = array
          .map(item => this.getPropertyValue(item, propertyName))
          .filter(v => typeof v === "number");
        return maxValues.length > 0 ? Math.max(...maxValues) : null;
      }
      case "someGreaterThan":
        return array.some(item => {
          const val = this.getPropertyValue(item, propertyName);
          return typeof val === "number" && val > (value as number);
        });
      case "someLessThan":
        return array.some(item => {
          const val = this.getPropertyValue(item, propertyName);
          return typeof val === "number" && val < (value as number);
        });
      case "everyGreaterThan":
        return array.every(item => {
          const val = this.getPropertyValue(item, propertyName);
          return typeof val === "number" && val > (value as number);
        });
      case "everyLessThan":
        return array.every(item => {
          const val = this.getPropertyValue(item, propertyName);
          return typeof val === "number" && val < (value as number);
        });
      case "map":
        return array.map(item => this.getPropertyValue(item, propertyName));
      case "distinct": {
        const distinctValues = array.map(item => this.getPropertyValue(item, propertyName));
        return [...new Set(distinctValues)];
      }
      case "first":
        return array.length > 0 ? array[0] : null;
      case "last":
        return array.length > 0 ? array[array.length - 1] : null;
      default:
        throw new RuntimeValidationError(`Unknown array method: ${methodName}`, {
          operation: "array_method_evaluation",
          field: "method",
          value: methodName,
        });
    }
  }

  private handleEmptyArray(method: string): unknown {
    const boolFalse = [
      "someEqual",
      "someContains",
      "has",
      "hasContains",
      "someGreaterThan",
      "someLessThan",
    ];
    const boolTrue = ["everyEqual", "everyHas", "everyGreaterThan", "everyLessThan"];
    const zero = ["countWhere", "countWhereContains", "sum"];
    const nullResult = ["findEqual", "findContains", "first", "last", "min", "max"];
    const emptyArray = ["map", "distinct"];

    if (boolFalse.includes(method)) return false;
    if (boolTrue.includes(method)) return true;
    if (zero.includes(method)) return 0;
    if (nullResult.includes(method)) return null;
    if (emptyArray.includes(method)) return [];
    if (method === "avg") return 0;
    return false;
  }

  private getPropertyValue(obj: unknown, propertyPath: string): unknown {
    if (typeof obj !== "object" || obj === null) {
      return undefined;
    }
    const parts = propertyPath.split(".");
    let current: unknown = obj;
    for (const part of parts) {
      if (typeof current !== "object" || current === null) {
        return undefined;
      }
      current = (current as Record<string, unknown>)[part];
      if (current === undefined) {
        return undefined;
      }
    }
    return current;
  }

  private evaluateArrayLiteral(node: ArrayLiteralExpr, context: EvaluationContext): unknown[] {
    return node.elements.map(el => this.evaluateAST(el, context));
  }

  private evaluateMemberAccess(node: MemberAccessExpr, context: EvaluationContext): unknown {
    const obj = this.evaluateAST(node.object, context);

    if (obj === null || obj === undefined) {
      this.logger.warn(`Cannot access property '${node.property}' on ${obj}`, {
        property: node.property,
      });
      return undefined;
    }

    if (typeof obj === "string" || typeof obj === "number" || typeof obj === "boolean") {
      return (obj as unknown as Record<string, unknown>)[node.property];
    }

    if (typeof obj !== "object") {
      this.logger.warn(`Cannot access property '${node.property}' on non-object: ${typeof obj}`, {
        property: node.property,
        objectType: typeof obj,
      });
      return undefined;
    }

    if (
      node.property === "__proto__" ||
      node.property === "constructor" ||
      node.property === "prototype"
    ) {
      this.logger.warn(`Blocked access to forbidden property: ${node.property}`);
      throw new ExpressionSecurityError(
        `Access to forbidden property '${node.property}' is not allowed`,
        { operation: "member_access_evaluation", field: "property", value: node.property },
      );
    }

    const numericIndex = Number(node.property);
    if (Array.isArray(obj) && Number.isInteger(numericIndex) && numericIndex >= 0) {
      validateArrayIndex(obj, numericIndex);
    }

    const result = (obj as Record<string, unknown>)[node.property];
    this.logger.debug(`Member access: ${node.property} = ${result}`, {
      property: node.property,
      resultType: typeof result,
    });
    return result;
  }

  registerFunction(name: string, fn: (...args: unknown[]) => unknown): void {
    this.registeredFunctions.set(name, fn);
    this.logger.info(`Registered custom function: ${name}`);
  }

  unregisterFunction(name: string): void {
    this.registeredFunctions.delete(name);
    this.logger.info(`Unregistered custom function: ${name}`);
  }

  getRegisteredFunctions(): string[] {
    return Array.from(this.registeredFunctions.keys());
  }

  private cleanArrayMethodCache(now: number): void {
    for (const [key, entry] of this.arrayMethodCache.entries()) {
      if (now - entry.timestamp > this.ARRAY_METHOD_CACHE_TTL) {
        this.arrayMethodCache.delete(key);
      }
    }
  }
}

export const expressionConditionExecutor = new ExpressionConditionExecutor();
