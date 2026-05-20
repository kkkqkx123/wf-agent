/**
 * ExpressionCompiler - 表达式编译器
 * Provides expression compilation and caching functionality to improve performance for repeated evaluations.
 */

import type { EvaluationContext } from "@wf-agent/types";
import { parseAST } from "./expression-parser.js";
import { expressionEvaluator } from "./expression-evaluator.js";
import type { ASTNode } from "./ast-types.js";

/**
 * Compiled Expression Interface
 */
export interface CompiledExpression {
  ast: ASTNode;
  evaluate: (context: EvaluationContext) => unknown;
  dependencies: string[];  // List of variables accessed
  complexity: number;      // Estimated computational cost
}

/**
 * Expression Compiler
 * Compiles expressions into optimized executable form with caching
 */
export class ExpressionCompiler {
  private cache = new Map<string, CompiledExpression>();
  
  /**
   * Compile an expression
   * @param expression Expression string
   * @returns Compiled expression
   */
  compile(expression: string): CompiledExpression {
    // Check cache first
    if (this.cache.has(expression)) {
      return this.cache.get(expression)!;
    }
    
    const ast = parseAST(expression);
    const dependencies = this.extractDependencies(ast);
    const complexity = this.calculateComplexity(ast);
    
    // Generate optimized evaluator
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
  
  /**
   * Clear the compilation cache
   */
  clearCache(): void {
    this.cache.clear();
  }
  
  /**
   * Get cache size
   */
  getCacheSize(): number {
    return this.cache.size;
  }
  
  /**
   * Extract variable dependencies from AST
   * @param node AST node
   * @returns Array of variable paths
   */
  private extractDependencies(node: ASTNode): string[] {
    const deps = new Set<string>();
    this.collectDependencies(node, deps);
    return Array.from(deps);
  }
  
  /**
   * Recursively collect dependencies from AST
   */
  private collectDependencies(node: ASTNode, deps: Set<string>): void {
    switch (node.type) {
      case "comparison":
        // Check if this is a function call comparison
        if (node.variablePath.startsWith("__FUNCTION_CALL__:")) {
          try {
            const functionNodeJson = node.variablePath.substring(18);
            const functionNode = JSON.parse(functionNodeJson) as ASTNode;
            this.collectDependencies(functionNode, deps);
          } catch (e) {
            // If parsing fails, just skip
          }
        } else if (node.variablePath.startsWith("__MEMBER_ACCESS__:")) {
          // For member access, extract root variable
          try {
            const memberAccessNodeJson = node.variablePath.substring(18);
            const memberAccessNode = JSON.parse(memberAccessNodeJson) as ASTNode;
            this.extractRootVariable(memberAccessNode, deps);
          } catch (e) {
            // If parsing fails, just skip
          }
        } else if (!node.variablePath.startsWith("__ARRAY_METHOD__:")) {
          deps.add(node.variablePath);
        }
        break;
      
      case "logical":
        this.collectDependencies(node.left, deps);
        this.collectDependencies(node.right, deps);
        break;
      
      case "not":
        this.collectDependencies(node.operand, deps);
        break;
      
      case "arithmetic":
        this.collectDependencies(node.left, deps);
        this.collectDependencies(node.right, deps);
        break;
      
      case "stringMethod":
        deps.add(node.variablePath);
        break;
      
      case "ternary":
        this.collectDependencies(node.condition, deps);
        this.collectDependencies(node.consequent, deps);
        this.collectDependencies(node.alternate, deps);
        break;
      
      case "arrayMethod":
        deps.add(node.arrayPath);
        break;
      
      case "arrayMethodComparison":
        this.collectDependencies(node.methodNode, deps);
        break;
      
      case "functionCall":
        // Collect dependencies from function arguments
        node.arguments.forEach(arg => this.collectDependencies(arg, deps));
        break;
      
      case "memberAccess":
        // For member access, extract the root variable from the object
        // e.g., user.address.city should extract 'user' as dependency
        this.extractRootVariable(node.object, deps);
        break;
      
      // Literal nodes have no dependencies
      case "boolean":
      case "number":
      case "string":
      case "null":
        break;
      
      default:
        // Unknown node types - log warning but don't crash
        break;
    }
  }
  
  /**
   * Extract the root variable from an AST node
   * For member access chains, extracts only the root variable name
   * @param node AST node
   * @param deps Set to add dependencies to
   */
  private extractRootVariable(node: ASTNode, deps: Set<string>): void {
    if (node.type === "memberAccess") {
      // Recursively extract from the object part
      this.extractRootVariable(node.object, deps);
    } else if (node.type === "comparison") {
      // For comparison nodes, check if it's a member access or simple variable
      if (node.variablePath.startsWith("__MEMBER_ACCESS__:")) {
        try {
          const memberAccessNode = JSON.parse(node.variablePath.substring(18)) as ASTNode;
          this.extractRootVariable(memberAccessNode, deps);
        } catch (e) {
          // If parsing fails, skip
        }
      } else if (!node.variablePath.startsWith("__FUNCTION_CALL__:") && 
                 !node.variablePath.startsWith("__ARRAY_METHOD__:")) {
        // Simple variable path - extract root (first part before dot)
        const rootVar = node.variablePath.split('.')[0];
        if (rootVar) deps.add(rootVar);
      }
    } else if (node.type === "stringMethod") {
      // For string methods, extract root variable
      const rootVar = node.variablePath.split('.')[0];
      if (rootVar) deps.add(rootVar);
    } else if (node.type === "arrayMethod") {
      // For array methods, extract root variable
      const rootVar = node.arrayPath.split('.')[0];
      if (rootVar) deps.add(rootVar);
    }
    // For other node types, recursively collect all dependencies
    else {
      this.collectDependencies(node, deps);
    }
  }
  
  /**
   * Calculate expression complexity (estimated computational cost)
   * @param node AST node
   * @returns Complexity score
   */
  private calculateComplexity(node: ASTNode): number {
    switch (node.type) {
      case "boolean":
      case "number":
      case "string":
      case "null":
        return 1;
      
      case "comparison":
        return 2;
      
      case "not":
        return 1 + this.calculateComplexity(node.operand);
      
      case "logical":
        return 2 + this.calculateComplexity(node.left) + this.calculateComplexity(node.right);
      
      case "arithmetic":
        return 2 + this.calculateComplexity(node.left) + this.calculateComplexity(node.right);
      
      case "stringMethod":
        return 3;
      
      case "ternary":
        return 3 + this.calculateComplexity(node.condition) + 
               Math.max(this.calculateComplexity(node.consequent), this.calculateComplexity(node.alternate));
      
      case "arrayMethod":
        return 5; // Array operations are more complex
      
      case "arrayMethodComparison":
        return 6 + this.calculateComplexity(node.methodNode);
      
      case "functionCall":
        // Function calls have base complexity plus argument complexity
        const argComplexity = node.arguments.reduce((sum, arg) => sum + this.calculateComplexity(arg), 0);
        return 4 + argComplexity;
      
      case "memberAccess":
        // Member access has low complexity - just property lookup
        // But nested access should have higher complexity
        return 2 + this.calculateComplexity(node.object);
      
      default:
        return 1;
    }
  }
}

// Export a singleton instance
export const expressionCompiler = new ExpressionCompiler();
