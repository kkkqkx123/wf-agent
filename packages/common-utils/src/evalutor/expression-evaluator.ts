/**
 * ExpressionEvaluator - Expression Evaluator
 * Provides the functionality to evaluate expressions, supporting the recursive evaluation of AST (Abstract Syntax Tree) nodes.
 */

import type { EvaluationContext } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { validatePath } from "./security-validator.js";
import { resolvePath } from "./path-resolver.js";
import { getGlobalLogger } from "../logger/logger.js";
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
import { parseAST } from "./expression-parser.js";

/**
 * Expression evaluator
 */
export class ExpressionEvaluator {
  private logger = getGlobalLogger().child("ExpressionEvaluator", { pkg: "common-utils" });

  /**
   * Evaluation Expression
   * @param expression: The expression string
   * @param context: The evaluation context
   * @returns: The evaluation result
   */
  evaluate(expression: string, context: EvaluationContext): unknown {
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
  private evaluateComparison(node: ComparisonNode, context: EvaluationContext): boolean {
    const variableValue = this.getVariableValue(node.variablePath, context);

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
}

// Export a singleton instance
export const expressionEvaluator = new ExpressionEvaluator();
