/**
 * ExpressionEvaluator - Expression Evaluator
 * Provides the functionality to evaluate expressions, supporting the recursive evaluation of AST (Abstract Syntax Tree) nodes.
 */

import type { EvaluationContext } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { validatePath } from "./security-validator.js";
import { resolvePath } from "./path-resolver.js";
import { getGlobalLogger } from "../logger/index.js";
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
  FunctionCallNode,
  MemberAccessNode,
  ArrayMethodName,
} from "./ast-types.js";
import { parseAST } from "./expression-parser.js";
import { validateComparisonTypes } from "./type-validator.js";

/**
 * Expression evaluator
 */
export class ExpressionEvaluator {
  private logger = getGlobalLogger().child("ExpressionEvaluator", { pkg: "common-utils" });
  
  // Cache for array method results
  private cache = new Map<string, { value: unknown; timestamp: number }>();
  private readonly CACHE_TTL = 1000; // 1 second TTL
  
  // Registered custom functions
  private registeredFunctions = new Map<string, (...args: any[]) => any>();

  /**
   * Evaluation Expression
   * @param expression: The expression string
   * @param context: The evaluation context
   * @returns: The evaluation result
   */
  evaluate(expression: string, context: EvaluationContext): unknown {
    // Clear cache before each new evaluation to avoid stale data
    this.cache.clear();
    
    // Parse into AST
    const ast = parseAST(expression);

    // Evaluating the AST
    return this.evaluateAST(ast, context);
  }

  /**
   * Evaluating an AST node
   * @param node: The AST node
   * @param context: The evaluation context
   * @returns: The evaluation result
   */
  evaluateAST(node: ASTNode, context: EvaluationContext): unknown {
    switch (node.type) {
      case "boolean":
        return this.evaluateBooleanLiteral(node as BooleanLiteralNode);

      case "number":
        return this.evaluateNumberLiteral(node as NumberLiteralNode);

      case "string":
        return this.evaluateStringLiteral(node as StringLiteralNode);

      case "null":
        return this.evaluateNullLiteral(node as NullLiteralNode);

      case "comparison":
        return this.evaluateComparison(node as ComparisonNode, context);

      case "logical":
        return this.evaluateLogical(node as LogicalNode, context);

      case "not":
        return this.evaluateNot(node as NotNode, context);

      case "arithmetic":
        return this.evaluateArithmetic(node as ArithmeticNode, context);

      case "stringMethod":
        return this.evaluateStringMethod(node as StringMethodNode, context);

      case "ternary":
        return this.evaluateTernary(node as TernaryNode, context);

      case "arrayMethod":
        return this.evaluateArrayMethod(node as ArrayMethodNode, context);

      case "arrayMethodComparison":
        return this.evaluateArrayMethodComparison(node as ArrayMethodComparisonNode, context);

      case "functionCall":
        return this.evaluateFunctionCall(node as FunctionCallNode, context);

      case "memberAccess":
        return this.evaluateMemberAccess(node as MemberAccessNode, context);

      default:
        throw new RuntimeValidationError(
          `Unknown AST node type: ${(node as { type?: string }).type}`,
          {
            operation: "ast_evaluation",
            field: "node",
            value: node,
          },
        );
    }
  }

  /**
   * Evaluating Boolean Literals
   */
  private evaluateBooleanLiteral(node: BooleanLiteralNode): boolean {
    return node.value;
  }

  /**
   * Evaluating numeric literals
   */
  private evaluateNumberLiteral(node: NumberLiteralNode): number {
    return node.value;
  }

  /**
   * Evaluating string literals
   */
  private evaluateStringLiteral(node: StringLiteralNode): string {
    return node.value;
  }

  /**
   * Evaluating the null literal
   */
  private evaluateNullLiteral(node: NullLiteralNode): null {
    return node.value;
  }

  /**
   * Evaluate and compare operations
   */
  private evaluateComparison(node: ComparisonNode, context: EvaluationContext): unknown {
    // Check if this is a member access expression
    if (node.variablePath.startsWith('__MEMBER_ACCESS__:')) {
      const memberAccessNodeJson = node.variablePath.substring(18); // Remove '__MEMBER_ACCESS__:'
      const memberAccessNode = JSON.parse(memberAccessNodeJson) as MemberAccessNode;
      
      // Evaluate the member access
      const memberValue = this.evaluateMemberAccess(memberAccessNode, context);
      
      // Handle special case: if value has __resolveVariable flag, return the member value directly
      if (
        node.value &&
        typeof node.value === "object" &&
        (node.value as Record<string, unknown>)["__resolveVariable"]
      ) {
        return memberValue;
      }
      
      // Otherwise perform comparison
      const compareValue = node.value;
      switch (node.operator) {
        case "==":
          return memberValue === compareValue;
        case "!=":
          return memberValue !== compareValue;
        case ">":
          return typeof memberValue === 'number' && typeof compareValue === 'number' && memberValue > compareValue;
        case "<":
          return typeof memberValue === 'number' && typeof compareValue === 'number' && memberValue < compareValue;
        case ">=":
          return typeof memberValue === 'number' && typeof compareValue === 'number' && memberValue >= compareValue;
        case "<=":
          return typeof memberValue === 'number' && typeof compareValue === 'number' && memberValue <= compareValue;
        default:
          this.logger.warn(`Operator ${node.operator} not supported for member access comparison`);
          return false;
      }
    }
    
    // Check if this is a function call expression
    if (node.variablePath.startsWith('__FUNCTION_CALL__:')) {
      const functionNodeJson = node.variablePath.substring(18); // Remove '__FUNCTION_CALL__:'
      const functionNode = JSON.parse(functionNodeJson) as FunctionCallNode;
      
      // Evaluate the function call
      const functionResult = this.evaluateFunctionCall(functionNode, context);
      const compareValue = node.value;
      
      // Perform the comparison
      switch (node.operator) {
        case "==":
          return functionResult === compareValue;
        case "!=":
          return functionResult !== compareValue;
        case ">":
          return typeof functionResult === 'number' && typeof compareValue === 'number' && functionResult > compareValue;
        case "<":
          return typeof functionResult === 'number' && typeof compareValue === 'number' && functionResult < compareValue;
        case ">=":
          return typeof functionResult === 'number' && typeof compareValue === 'number' && functionResult >= compareValue;
        case "<=":
          return typeof functionResult === 'number' && typeof compareValue === 'number' && functionResult <= compareValue;
        default:
          this.logger.warn(`Operator ${node.operator} not supported for function call comparison`);
          return false;
      }
    }
    
    // Check if this is an array method expression
    if (node.variablePath.startsWith('__ARRAY_METHOD__:')) {
      const methodExpression = node.variablePath.substring(15); // Remove '__ARRAY_METHOD__:'
      const methodAst = parseAST(methodExpression);
      
      if (methodAst.type === 'arrayMethod') {
        const methodResult = this.evaluateArrayMethod(methodAst as ArrayMethodNode, context);
        const compareValue = node.value;
        
        // Perform the comparison
        switch (node.operator) {
          case "==":
            return methodResult === compareValue;
          case "!=":
            return methodResult !== compareValue;
          case ">":
            return typeof methodResult === 'number' && typeof compareValue === 'number' && methodResult > compareValue;
          case "<":
            return typeof methodResult === 'number' && typeof compareValue === 'number' && methodResult < compareValue;
          case ">=":
            return typeof methodResult === 'number' && typeof compareValue === 'number' && methodResult >= compareValue;
          case "<=":
            return typeof methodResult === 'number' && typeof compareValue === 'number' && methodResult <= compareValue;
          default:
            this.logger.warn(`Operator ${node.operator} not supported for array method comparison`);
            return false;
        }
      }
    }
    
    const variableValue = this.getVariableValue(node.variablePath, context);

    // Handle special case: if value has __resolveVariable flag, return the variable value directly
    if (
      node.value &&
      typeof node.value === "object" &&
      (node.value as Record<string, unknown>)["__resolveVariable"]
    ) {
      return variableValue;
    }

    // Handle variable references
    let compareValue = node.value;
    if (
      compareValue &&
      typeof compareValue === "object" &&
      (compareValue as Record<string, unknown>)["__isVariableRef"]
    ) {
      compareValue = this.getVariableValue(
        (compareValue as Record<string, unknown>)["path"] as string,
        context,
      );
    }

    // If the variable does not exist, record a warning log but do not return false, allowing the comparison operator to proceed as normal.
    if (variableValue === undefined) {
      this.logger.warn(`Variable not found in condition evaluation: ${node.variablePath}`, {
        variablePath: node.variablePath,
        operator: node.operator,
        compareValue,
      });
    }

    switch (node.operator) {
      case "==":
        return variableValue === compareValue;
      case "!=":
        return variableValue !== compareValue;
      case ">":
        if (typeof variableValue !== "number" || typeof compareValue !== "number") {
          this.logger.warn(
            `Type mismatch in comparison: ${node.variablePath} (${typeof variableValue}) > ${typeof compareValue}`,
            { variablePath: node.variablePath, variableValue, compareValue },
          );
          return false;
        }
        return variableValue > compareValue;
      case "<":
        if (typeof variableValue !== "number" || typeof compareValue !== "number") {
          this.logger.warn(
            `Type mismatch in comparison: ${node.variablePath} (${typeof variableValue}) < ${typeof compareValue}`,
            { variablePath: node.variablePath, variableValue, compareValue },
          );
          return false;
        }
        return variableValue < compareValue;
      case ">=":
        if (typeof variableValue !== "number" || typeof compareValue !== "number") {
          this.logger.warn(
            `Type mismatch in comparison: ${node.variablePath} (${typeof variableValue}) >= ${typeof compareValue}`,
            { variablePath: node.variablePath, variableValue, compareValue },
          );
          return false;
        }
        return variableValue >= compareValue;
      case "<=":
        if (typeof variableValue !== "number" || typeof compareValue !== "number") {
          this.logger.warn(
            `Type mismatch in comparison: ${node.variablePath} (${typeof variableValue}) <= ${typeof compareValue}`,
            { variablePath: node.variablePath, variableValue, compareValue },
          );
          return false;
        }
        return variableValue <= compareValue;
      case "contains":
        return String(variableValue).includes(String(compareValue));
      case "in":
        if (!Array.isArray(compareValue)) {
          this.logger.warn(
            `Right operand of 'in' operator must be an array: ${typeof compareValue}`,
            { variablePath: node.variablePath, compareValue },
          );
          return false;
        }
        
        // ENHANCED: Support checking if object's common properties are in array
        if (typeof variableValue === 'object' && variableValue !== null && !Array.isArray(variableValue)) {
          // Try common property names for message-like objects
          const commonProps = ['role', 'name', 'type', 'id', 'status', 'nodeType'];
          for (const prop of commonProps) {
            if (prop in variableValue && compareValue.includes((variableValue as Record<string, unknown>)[prop])) {
              return true;
            }
          }
          return false;
        }
        
        // Original behavior for primitive values
        return compareValue.includes(variableValue);
      default:
        throw new RuntimeValidationError(`Unknown operator "${node.operator}"`, {
          operation: "comparison_evaluation",
          field: "operator",
          value: node.operator,
          context: {
            variablePath: node.variablePath,
            variableValue,
            compareValue,
          },
        });
    }
  }

  /**
   * Evaluate logical operations
   */
  private evaluateLogical(node: LogicalNode, context: EvaluationContext): boolean {
    const leftResult = this.evaluateAST(node.left, context);

    // Short-circuit evaluation
    if (node.operator === "&&" && !leftResult) {
      return false;
    }
    if (node.operator === "||" && leftResult) {
      return true;
    }

    const rightResult = this.evaluateAST(node.right, context);

    if (node.operator === "&&") {
      return Boolean(leftResult && rightResult);
    } else {
      return Boolean(leftResult || rightResult);
    }
  }

  /**
   * Evaluate the NOT operation
   */
  private evaluateNot(node: NotNode, context: EvaluationContext): boolean {
    const operandResult = this.evaluateAST(node.operand, context);
    return Boolean(!operandResult);
  }

  /**
   * Evaluating Arithmetic Operations
   */
  private evaluateArithmetic(node: ArithmeticNode, context: EvaluationContext): number {
    const leftValue = this.evaluateAST(node.left, context);
    const rightValue = this.evaluateAST(node.right, context);

    if (typeof leftValue !== "number" || typeof rightValue !== "number") {
      this.logger.warn(
        `Type mismatch in arithmetic operation: ${typeof leftValue} ${node.operator} ${typeof rightValue}`,
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

  /**
   * Evaluate String Methods
   */
  private evaluateStringMethod(node: StringMethodNode, context: EvaluationContext): unknown {
    const stringValue = this.getVariableValue(node.variablePath, context);

    if (typeof stringValue !== "string") {
      this.logger.warn(`String method called on non-string value: ${typeof stringValue}`, {
        variablePath: node.variablePath,
        value: stringValue,
        method: node.method,
      });
      return false;
    }

    switch (node.method) {
      case "startsWith":
        return stringValue.startsWith(String(node.argument || ""));
      case "endsWith":
        return stringValue.endsWith(String(node.argument || ""));
      case "length":
        return stringValue.length;
      case "toLowerCase":
        return stringValue.toLowerCase();
      case "toUpperCase":
        return stringValue.toUpperCase();
      case "trim":
        return stringValue.trim();
      default:
        throw new RuntimeValidationError(`Unknown string method: ${node.method}`, {
          operation: "string_method_evaluation",
          field: "method",
          value: node.method,
        });
    }
  }

  /**
   * Evaluating the ternary operator
   */
  private evaluateTernary(node: TernaryNode, context: EvaluationContext): unknown {
    const conditionResult = this.evaluateAST(node.condition, context);

    if (conditionResult) {
      return this.evaluateAST(node.consequent, context);
    } else {
      return this.evaluateAST(node.alternate, context);
    }
  }

  /**
   * Evaluate Array Methods
   */
  private evaluateArrayMethod(node: ArrayMethodNode, context: EvaluationContext): unknown {
    // Create cache key from method, path, and arguments
    const cacheKey = `${node.method}:${node.arrayPath}:${node.propertyName}:${JSON.stringify(node.value)}`;
    
    // Check cache first
    const cached = this.cache.get(cacheKey);
    const now = Date.now();
    
    if (cached && (now - cached.timestamp) < this.CACHE_TTL) {
      this.logger.debug(`Cache hit for array method: ${cacheKey}`);
      return cached.value;
    }
    
    // Compute result
    const result = this.computeArrayMethod(node, context);
    
    // Store in cache
    this.cache.set(cacheKey, { value: result, timestamp: now });
    
    // Clean old entries if cache is too large
    if (this.cache.size > 100) {
      this.cleanCache(now);
    }
    
    return result;
  }
  
  /**
   * Compute array method result (actual implementation)
   */
  private computeArrayMethod(node: ArrayMethodNode, context: EvaluationContext): unknown {
    const array = this.getVariableValue(node.arrayPath, context);
    
    if (array === undefined || array === null) {
      this.logger.warn(
        `Array path '${node.arrayPath}' resolved to ${array === undefined ? 'undefined' : 'null'}`,
        { 
          arrayPath: node.arrayPath, 
          method: node.method,
          contextKeys: Object.keys(context)
        }
      );
      return this.handleEmptyArray(node.method);
    }
    
    if (!Array.isArray(array)) {
      this.logger.error(
        `Expected array for method '${node.method}', got ${typeof array}`,
        { 
          arrayPath: node.arrayPath, 
          method: node.method,
          actualType: typeof array,
          actualValue: array
        }
      );
      return false;
    }
    
    // Validate arguments
    this.validateArrayMethodArgs(node.method, node.propertyName, node.value);
    
    if (array.length === 0) {
      return this.handleEmptyArray(node.method);
    }
    
    switch (node.method) {
      case 'someEqual':
        return array.some(item => this.getPropertyValue(item, node.propertyName) === node.value);
      
      case 'someContains':
        return array.some(item => {
          const propValue = this.getPropertyValue(item, node.propertyName);
          return typeof propValue === 'string' && propValue.includes(String(node.value));
        });
      
      case 'everyEqual':
        return array.every(item => this.getPropertyValue(item, node.propertyName) === node.value);
      
      case 'everyHas':
        return array.every(item => node.propertyName in item && item[node.propertyName] != null);
      
      case 'countWhere':
        return array.filter(item => this.getPropertyValue(item, node.propertyName) === node.value).length;
      
      case 'countWhereContains':
        return array.filter(item => {
          const propValue = this.getPropertyValue(item, node.propertyName);
          return typeof propValue === 'string' && propValue.includes(String(node.value));
        }).length;
      
      case 'findEqual':
        return array.find(item => this.getPropertyValue(item, node.propertyName) === node.value) ?? null;
      
      case 'findContains':
        return array.find(item => {
          const propValue = this.getPropertyValue(item, node.propertyName);
          return typeof propValue === 'string' && propValue.includes(String(node.value));
        }) ?? null;
      
      case 'has':
        return array.some(item => this.getPropertyValue(item, node.propertyName) === node.value);
      
      case 'hasContains':
        return array.some(item => {
          const propValue = this.getPropertyValue(item, node.propertyName);
          return typeof propValue === 'string' && propValue.includes(String(node.value));
        });
      
      // Phase 3.1: Aggregation functions
      case 'sum':
        return array.reduce((acc, item) => {
          const val = this.getPropertyValue(item, node.propertyName);
          return typeof val === 'number' ? acc + val : acc;
        }, 0);
      
      case 'avg': {
        const numericValues = array.map(item => this.getPropertyValue(item, node.propertyName))
                                    .filter(v => typeof v === 'number');
        if (numericValues.length === 0) {
          return 0;
        }
        const sum = numericValues.reduce((acc, val) => acc + val, 0);
        return sum / numericValues.length;
      }
      
      case 'min': {
        const values = array.map(item => this.getPropertyValue(item, node.propertyName))
                            .filter(v => typeof v === 'number');
        return values.length > 0 ? Math.min(...values) : null;
      }
      
      case 'max': {
        const maxValues = array.map(item => this.getPropertyValue(item, node.propertyName))
                               .filter(v => typeof v === 'number');
        return maxValues.length > 0 ? Math.max(...maxValues) : null;
      }
      
      // Phase 3.2: Comparison-based filters
      case 'someGreaterThan':
        return array.some(item => {
          const val = this.getPropertyValue(item, node.propertyName);
          return typeof val === 'number' && val > (node.value as number);
        });
      
      case 'someLessThan':
        return array.some(item => {
          const val = this.getPropertyValue(item, node.propertyName);
          return typeof val === 'number' && val < (node.value as number);
        });
      
      case 'everyGreaterThan':
        return array.every(item => {
          const val = this.getPropertyValue(item, node.propertyName);
          return typeof val === 'number' && val > (node.value as number);
        });
      
      case 'everyLessThan':
        return array.every(item => {
          const val = this.getPropertyValue(item, node.propertyName);
          return typeof val === 'number' && val < (node.value as number);
        });
      
      // Phase 3.4: Array transformation methods
      case 'map':
        return array.map(item => this.getPropertyValue(item, node.propertyName));
      
      case 'distinct': {
        const distinctValues = array.map(item => this.getPropertyValue(item, node.propertyName));
        return [...new Set(distinctValues)];
      }
      
      case 'first':
        return array.length > 0 ? array[0] : null;
      
      case 'last':
        return array.length > 0 ? array[array.length - 1] : null;
      
      default:
        throw new RuntimeValidationError(`Unknown array method: ${node.method}`, {
          operation: "array_method_evaluation",
          field: "method",
          value: node.method
        });
    }
  }
  
  /**
   * Clean expired cache entries
   */
  private cleanCache(now: number): void {
    for (const [key, entry] of this.cache.entries()) {
      if (now - entry.timestamp > this.CACHE_TTL) {
        this.cache.delete(key);
      }
    }
  }

  /**
   * Evaluate Array Method Comparison
   * Handles expressions like: input.messages.countWhere('role', 'user') > 5
   */
  private evaluateArrayMethodComparison(node: ArrayMethodComparisonNode, context: EvaluationContext): boolean {
    // First evaluate the array method
    const methodResult = this.evaluateArrayMethod(node.methodNode, context);
    
    // Handle variable references in compare value
    let compareValue = node.compareValue;
    if (compareValue && typeof compareValue === "object" && 
        (compareValue as Record<string, unknown>)["__isVariableRef"]) {
      compareValue = this.getVariableValue(
        (compareValue as Record<string, unknown>)["path"] as string,
        context
      );
    }
    
    // Validate types before comparison
    if (!validateComparisonTypes(node.operator, methodResult, compareValue, {
      method: node.methodNode.method,
      propertyPath: node.methodNode.arrayPath
    })) {
      return false;
    }
    
    // Perform comparison based on operator
    switch (node.operator) {
      case "==":
        return methodResult === compareValue;
      case "!=":
        return methodResult !== compareValue;
      case ">":
        return typeof methodResult === 'number' && typeof compareValue === 'number' && 
               methodResult > compareValue;
      case "<":
        return typeof methodResult === 'number' && typeof compareValue === 'number' && 
               methodResult < compareValue;
      case ">=":
        return typeof methodResult === 'number' && typeof compareValue === 'number' && 
               methodResult >= compareValue;
      case "<=":
        return typeof methodResult === 'number' && typeof compareValue === 'number' && 
               methodResult <= compareValue;
      default:
        this.logger.warn(`Unsupported operator ${node.operator} for array method comparison`);
        return false;
    }
  }

  /**
   * Validate array method arguments
   */
  private validateArrayMethodArgs(method: ArrayMethodName, propertyName: string, value?: unknown): void {
    // Methods that don't require property name
    const noArgMethods = ['first', 'last'];
    if (noArgMethods.includes(method)) {
      return;
    }
    
    if (!propertyName || propertyName.trim() === '') {
      throw new RuntimeValidationError(
        `Property name cannot be empty for method ${method}`,
        { 
          operation: "validate_array_method_args",
          context: { method, propertyName }
        }
      );
    }
    
    // Type-specific validations
    if (method.includes('Contains') && value !== undefined) {
      if (typeof value !== 'string') {
        this.logger.warn(
          `Method ${method} expects string value for contains check, got ${typeof value}`,
          { method, value, valueType: typeof value }
        );
      }
    }
  }

  /**
   * Get property value from object
   * Supports nested property access using dot notation (e.g., 'metadata.tags.name')
   */
  private getPropertyValue(obj: unknown, propertyPath: string): unknown {
    if (typeof obj !== 'object' || obj === null) {
      return undefined;
    }
    
    // Support dot notation: metadata.tags.name
    const parts = propertyPath.split('.');
    let current: unknown = obj;
    
    for (const part of parts) {
      if (typeof current !== 'object' || current === null) {
        this.logger.debug(
          `Cannot access property '${part}' on non-object value`,
          { propertyPath, currentType: typeof current }
        );
        return undefined;
      }
      current = (current as Record<string, unknown>)[part];
      
      if (current === undefined) {
        this.logger.debug(
          `Property '${part}' not found in object`,
          { propertyPath, availableKeys: current && typeof current === 'object' ? Object.keys(current) : [] }
        );
        return undefined;
      }
    }
    
    return current;
  }

  /**
   * Handle empty array edge cases
   */
  private handleEmptyArray(method: ArrayMethodName): unknown {
    switch (method) {
      case 'someEqual':
      case 'someContains':
      case 'has':
      case 'hasContains':
      case 'someGreaterThan':
      case 'someLessThan':
        return false;  // No items match
      
      case 'everyEqual':
      case 'everyHas':
      case 'everyGreaterThan':
      case 'everyLessThan':
        return true;   // Vacuously true
      
      case 'countWhere':
      case 'countWhereContains':
      case 'sum':
        return 0;      // Zero matches or sum
      
      case 'findEqual':
      case 'findContains':
      case 'first':
      case 'last':
        return null;   // No item found
      
      case 'avg':
        return 0;   // Return 0 for empty arrays (more intuitive for average)
      
      case 'min':
      case 'max':
        return null;   // Cannot compute min/max on empty array
      
      case 'map':
      case 'distinct':
        return [];     // Empty array result
      
      default:
        return false;
    }
  }

  /**
   * Getting the value of a variable
   *
   * Data source access rules:
   * - Explicit prefixes: input.xxx, output.xxx, variables.xxx - get from the specified data source
   * - Simple variable names: xxx - only from variables (syntactic sugar, equivalent to variables.xxx)
   * - Other nested paths: user.name - get from variables (equivalent to variables.user.name)
   *
   * @param variablePath variablePath
   * @param context Evaluation context
   * @returns Variable value
   */
  private getVariableValue(variablePath: string, context: EvaluationContext): unknown {
    // Verify path security
    validatePath(variablePath);

    // Determine whether it is a nested path.
    const isNestedPath = variablePath.includes(".") || variablePath.includes("[");

    if (isNestedPath) {
      // Check if it starts with 'input.'
      if (variablePath.startsWith("input.")) {
        const subPath = variablePath.substring(6); // Remove 'input.'
        return resolvePath(subPath, context.input);
      }

      // Check if it starts with "output.".
      if (variablePath.startsWith("output.")) {
        const subPath = variablePath.substring(7); // Remove 'output.'
        return resolvePath(subPath, context.output);
      }

      // Check if it starts with "variables.".
      if (variablePath.startsWith("variables.")) {
        const subPath = variablePath.substring(10); // Remove 'variables.'
        return resolvePath(subPath, context.variables);
      }

      // Other nested paths: Obtained from variables (equivalent to variables.xxx)
      return resolvePath(variablePath, context.variables);
    } else {
      // Simple variable names: Retrieved only from `variables` (syntactic sugar, equivalent to `variables.xxx`).
      return context.variables[variablePath];
    }
  }

  /**
   * Evaluate Function Call
   * Supports custom function invocation for extensibility
   * @param node Function call AST node
   * @param context Evaluation context
   * @returns Function execution result
   */
  private evaluateFunctionCall(node: FunctionCallNode, context: EvaluationContext): unknown {
    const func = this.registeredFunctions.get(node.functionName);
    
    if (!func) {
      throw new RuntimeValidationError(
        `Unknown function: ${node.functionName}`,
        {
          operation: "function_call_evaluation",
          field: "functionName",
          value: node.functionName,
          context: {
            availableFunctions: Array.from(this.registeredFunctions.keys()),
          },
        },
      );
    }
    
    try {
      // Evaluate all arguments
      const args = node.arguments.map(arg => this.evaluateAST(arg, context));
      
      // Execute the function
      return func(...args);
    } catch (error) {
      this.logger.error(
        `Function execution failed: ${node.functionName}`,
        {
          functionName: node.functionName,
          error: error instanceof Error ? error.message : String(error),
        },
      );
      throw new RuntimeValidationError(
        `Function execution failed: ${node.functionName} - ${error instanceof Error ? error.message : String(error)}`,
        {
          operation: "function_call_execution",
          field: "functionName",
          value: node.functionName,
        },
      );
    }
  }

  /**
   * Register a custom function
   * @param name Function name
   * @param fn Function implementation
   */
  registerFunction(name: string, fn: (...args: any[]) => any): void {
    this.registeredFunctions.set(name, fn);
    this.logger.info(`Registered custom function: ${name}`);
  }

  /**
   * Evaluate Member Access (property access on objects)
   * Provides better static analysis and error messages for property access
   * @param node MemberAccessNode
   * @param context Evaluation context
   * @returns The accessed property value
   */
  private evaluateMemberAccess(node: MemberAccessNode, context: EvaluationContext): unknown {
    // First evaluate the object
    const obj = this.evaluateAST(node.object, context);
    
    // Check if object is null or undefined
    if (obj === null || obj === undefined) {
      this.logger.warn(
        `Cannot access property '${node.property}' on ${obj}`,
        { property: node.property, objectType: typeof obj }
      );
      return undefined;
    }
    
    // Check if object is actually an object type
    if (typeof obj !== 'object') {
      this.logger.warn(
        `Cannot access property '${node.property}' on non-object type: ${typeof obj}`,
        { property: node.property, objectType: typeof obj, value: obj }
      );
      return undefined;
    }
    
    // Security check: prevent prototype pollution
    if (node.property === '__proto__' || node.property === 'constructor' || node.property === 'prototype') {
      this.logger.warn(
        `Blocked access to forbidden property: ${node.property}`,
        { property: node.property }
      );
      throw new RuntimeValidationError(
        `Access to forbidden property '${node.property}' is not allowed`,
        {
          operation: "member_access_evaluation",
          field: "property",
          value: node.property,
        }
      );
    }
    
    // Access the property
    const result = (obj as Record<string, unknown>)[node.property];
    
    this.logger.debug(
      `Member access: ${node.property} = ${result}`,
      { property: node.property, resultType: typeof result }
    );
    
    return result;
  }

  /**
   * Unregister a custom function
   * @param name Function name
   */
  unregisterFunction(name: string): void {
    this.registeredFunctions.delete(name);
    this.logger.info(`Unregistered custom function: ${name}`);
  }

  /**
   * Get list of registered functions
   * @returns Array of registered function names
   */
  getRegisteredFunctions(): string[] {
    return Array.from(this.registeredFunctions.keys());
  }
}

// Export a singleton instance
export const expressionEvaluator = new ExpressionEvaluator();
