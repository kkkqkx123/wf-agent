/**
 * ExpressionCompiler - Expression Compiler
 * Compiles expressions into optimized executable form with caching.
 */

import type { EvaluationContext } from "@wf-agent/types";
import { dslParse } from "./dsl/index.js";
import { expressionEvaluator } from "./expression-evaluator.js";
import type { Expression, MemberAccessExpr, IdentifierExpr } from "./dsl/types.js";

export interface CompiledExpression {
  ast: Expression;
  evaluate: (context: EvaluationContext) => unknown;
  dependencies: string[];
  complexity: number;
}

export class ExpressionCompiler {
  private cache = new Map<string, CompiledExpression>();

  compile(expression: string): CompiledExpression {
    if (this.cache.has(expression)) {
      return this.cache.get(expression)!;
    }

    const ast = dslParse(expression);
    if (ast === null) {
      throw new Error(`Failed to parse expression: ${expression}`);
    }

    const dependencies = this.extractDependencies(ast);
    const complexity = this.calculateComplexity(ast);

    const evaluate = (context: EvaluationContext) => {
      return expressionEvaluator.evaluateAST(ast, context);
    };

    const compiled: CompiledExpression = {
      ast,
      evaluate,
      dependencies,
      complexity,
    };

    this.cache.set(expression, compiled);
    return compiled;
  }

  clearCache(): void {
    this.cache.clear();
  }

  getCacheSize(): number {
    return this.cache.size;
  }

  private extractDependencies(node: Expression): string[] {
    const deps = new Set<string>();
    this.collectDependencies(node, deps);
    return Array.from(deps);
  }

  private collectDependencies(node: Expression, deps: Set<string>): void {
    switch (node.type) {
      case "identifier":
        deps.add(node.name);
        break;

      case "memberAccess":
        deps.add(this.buildMemberAccessPath(node as MemberAccessExpr));
        this.collectDependencies(node.object, deps);
        break;

      case "binary":
        this.collectDependencies(node.left, deps);
        this.collectDependencies(node.right, deps);
        break;

      case "not":
        this.collectDependencies(node.operand, deps);
        break;

      case "unaryMinus":
        this.collectDependencies(node.operand, deps);
        break;

      case "ternary":
        this.collectDependencies(node.condition, deps);
        this.collectDependencies(node.consequent, deps);
        this.collectDependencies(node.alternate, deps);
        break;

      case "call":
        this.collectDependencies(node.callee, deps);
        node.arguments.forEach((arg) => this.collectDependencies(arg, deps));
        break;

      case "arrayLiteral":
        node.elements.forEach((el) => this.collectDependencies(el, deps));
        break;

      case "literal":
        break;

      default:
        break;
    }
  }

  private buildMemberAccessPath(node: MemberAccessExpr): string {
    const parts: string[] = [node.property];
    let current = node.object;
    while (current.type === "memberAccess") {
      parts.unshift((current as MemberAccessExpr).property);
      current = (current as MemberAccessExpr).object;
    }
    if (current.type === "identifier") {
      parts.unshift((current as IdentifierExpr).name);
    }
    return parts.join(".");
  }

  private calculateComplexity(node: Expression): number {
    switch (node.type) {
      case "literal":
        return 1;

      case "identifier":
        return 1;

      case "memberAccess":
        return 2 + this.calculateComplexity(node.object);

      case "binary":
        return 2 + this.calculateComplexity(node.left) + this.calculateComplexity(node.right);

      case "not":
        return 1 + this.calculateComplexity(node.operand);

      case "unaryMinus":
        return 1 + this.calculateComplexity(node.operand);

      case "ternary":
        return 3 + this.calculateComplexity(node.condition) + Math.max(this.calculateComplexity(node.consequent), this.calculateComplexity(node.alternate));

      case "call":
        const argComplexity = node.arguments.reduce((sum, arg) => sum + this.calculateComplexity(arg), 0);
        return 4 + this.calculateComplexity(node.callee) + argComplexity;

      case "arrayLiteral":
        return 1 + node.elements.reduce((sum, el) => sum + this.calculateComplexity(el), 0);

      default:
        return 1;
    }
  }
}

export const expressionCompiler = new ExpressionCompiler();