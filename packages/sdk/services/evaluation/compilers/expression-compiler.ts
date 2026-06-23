/**
 * ExpressionCompiler - Expression Compiler
 * Compiles expression strings into optimized executable form
 * Implements ICompiler interface
 * Note: Caching is handled by CacheManager, not by this class
 */

import { dslParse } from "../dsl/index.js";
import type { Expression, MemberAccessExpr, IdentifierExpr } from "../dsl/types.js";
import type { ICompiler, CompiledUnit } from "../types/index.js";

export class ExpressionCompiler implements ICompiler {
  compile(expression: string): CompiledUnit {
    const ast = dslParse(expression);

    const dependencies = this.extractDependencies(ast);
    const complexity = this.calculateComplexity(ast);

    const unit: CompiledUnit = {
      ast,
      dependencies,
      complexity,
      metadata: {
        type: "expression",
        expression,
      },
    };

    return unit;
  }

  clearCache(): void {
    // Caching is handled by CacheManager, nothing to clear here
  }

  getCacheSize(): number {
    // Caching is handled by CacheManager
    return 0;
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
        node.arguments.forEach((arg: Expression) => this.collectDependencies(arg, deps));
        break;

      case "arrayLiteral":
        node.elements.forEach((el: Expression) => this.collectDependencies(el, deps));
        break;

      case "literal":
        break;

      default:
        break;
    }
  }

  private buildMemberAccessPath(node: MemberAccessExpr): string {
    const isNumeric = (prop: string): boolean => /^\d+$/.test(prop);

    const parts: string[] = [];
    if (isNumeric(node.property)) {
      parts.push(`[${node.property}]`);
    } else {
      parts.push(node.property);
    }

    let current = node.object;
    while (current.type === "memberAccess") {
      const ma = current as MemberAccessExpr;
      if (isNumeric(ma.property)) {
        parts.unshift(`[${ma.property}]`);
      } else {
        parts.unshift(ma.property);
      }
      current = ma.object;
    }
    if (current.type === "identifier") {
      parts.unshift((current as IdentifierExpr).name);
    }

    let result = parts[0]!;
    for (let i = 1; i < parts.length; i++) {
      const part = parts[i]!;
      if (part.startsWith("[")) {
        result += part;
      } else {
        result += "." + part;
      }
    }
    return result;
  }

  private calculateComplexity(node: Expression): number {
    switch (node.type) {
      case "literal":
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
        return (
          3 +
          this.calculateComplexity(node.condition) +
          Math.max(
            this.calculateComplexity(node.consequent),
            this.calculateComplexity(node.alternate),
          )
        );

      case "call": {
        const argComplexity = node.arguments.reduce(
          (sum: number, arg: Expression) => sum + this.calculateComplexity(arg),
          0,
        );
        return 4 + this.calculateComplexity(node.callee) + argComplexity;
      }

      case "arrayLiteral":
        return 1 + node.elements.reduce((sum: number, el: Expression) => sum + this.calculateComplexity(el), 0);

      default:
        return 1;
    }
  }
}

export const expressionCompiler = new ExpressionCompiler();

