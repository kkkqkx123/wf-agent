/**
 * Dependency Tracking System
 * Enables intelligent re-evaluation based on variable changes
 */

import { expressionCompiler, type CompiledExpression } from "./expression-compiler.js";
import type { EvaluationContext } from "@wf-agent/types";

/**
 * Tracked expression with dependency information
 */
export interface TrackedExpression {
  /** The original expression string */
  expression: string;
  /** Compiled expression */
  compiled: CompiledExpression;
  /** List of variable dependencies */
  dependencies: string[];
  /** Last evaluation result */
  lastResult?: unknown;
  /** Last evaluation timestamp */
  lastEvaluatedAt?: number;
}

/**
 * Variable change tracker
 * Monitors which variables have changed since last check
 */
class VariableChangeTracker {
  private previousValues = new Map<string, unknown>();

  /**
   * Check if a variable has changed
   * @param variablePath Variable path to check
   * @param currentValue Current value
   * @returns true if value has changed
   */
  hasChanged(variablePath: string, currentValue: unknown): boolean {
    const previousValue = this.previousValues.get(variablePath);

    // First time seeing this variable - consider it changed
    if (!this.previousValues.has(variablePath)) {
      return true;
    }

    // Deep comparison for objects/arrays
    return !this.valuesEqual(previousValue, currentValue);
  }

  /**
   * Update tracked value for a variable
   * @param variablePath Variable path
   * @param value New value
   */
  update(variablePath: string, value: unknown): void {
    this.previousValues.set(variablePath, value);
  }

  /**
   * Clear all tracked values
   */
  clear(): void {
    this.previousValues.clear();
  }

  /**
   * Deep equality check
   */
  private valuesEqual(a: unknown, b: unknown): boolean {
    if (a === b) return true;
    if (a == null || b == null) return false;
    if (typeof a !== typeof b) return false;

    if (typeof a === "object") {
      const aObj = a as Record<string, unknown>;
      const bObj = b as Record<string, unknown>;

      const keysA = Object.keys(aObj);
      const keysB = Object.keys(bObj);

      if (keysA.length !== keysB.length) return false;

      return keysA.every(key => this.valuesEqual(aObj[key], bObj[key]));
    }

    return false;
  }
}

/**
 * Expression dependency manager
 * Manages multiple tracked expressions and their dependencies
 */
export class DependencyManager {
  private trackedExpressions = new Map<string, TrackedExpression>();
  private changeTracker = new VariableChangeTracker();

  /**
   * Register an expression for tracking
   * @param key Unique identifier for the expression
   * @param expression Expression string
   * @param context Initial evaluation context
   * @returns The tracked expression
   */
  register(key: string, expression: string, context: EvaluationContext): TrackedExpression {
    const compiled = expressionCompiler.compile(expression);
    const result = compiled.evaluate(context);

    const tracked: TrackedExpression = {
      expression,
      compiled,
      dependencies: compiled.dependencies,
      lastResult: result,
      lastEvaluatedAt: Date.now(),
    };

    this.trackedExpressions.set(key, tracked);

    // Track initial values of dependencies
    compiled.dependencies.forEach(dep => {
      const value = this.getVariableValue(dep, context);
      this.changeTracker.update(dep, value);
    });

    return tracked;
  }

  /**
   * Unregister a tracked expression
   * @param key Expression identifier
   */
  unregister(key: string): void {
    this.trackedExpressions.delete(key);
  }

  /**
   * Check if any dependencies have changed
   * @param key Expression identifier
   * @param context Current context
   * @returns true if any dependency has changed
   */
  hasDependenciesChanged(key: string, context: EvaluationContext): boolean {
    const tracked = this.trackedExpressions.get(key);
    if (!tracked) {
      throw new Error(`Expression not found: ${key}`);
    }

    return tracked.dependencies.some(dep => {
      const currentValue = this.getVariableValue(dep, context);
      return this.changeTracker.hasChanged(dep, currentValue);
    });
  }

  /**
   * Re-evaluate expression if dependencies have changed
   * @param key Expression identifier
   * @param context Current context
   * @returns Evaluation result (cached if no changes)
   */
  evaluateIfChanged(key: string, context: EvaluationContext): unknown {
    const tracked = this.trackedExpressions.get(key);
    if (!tracked) {
      throw new Error(`Expression not found: ${key}`);
    }

    // Check if re-evaluation is needed
    if (!this.hasDependenciesChanged(key, context)) {
      return tracked.lastResult;
    }

    // Re-evaluate
    const result = tracked.compiled.evaluate(context);
    tracked.lastResult = result;
    tracked.lastEvaluatedAt = Date.now();

    // Update tracked values
    tracked.dependencies.forEach(dep => {
      const value = this.getVariableValue(dep, context);
      this.changeTracker.update(dep, value);
    });

    return result;
  }

  /**
   * Get tracked expression info
   * @param key Expression identifier
   * @returns Tracked expression or undefined
   */
  getTrackedExpression(key: string): TrackedExpression | undefined {
    return this.trackedExpressions.get(key);
  }

  /**
   * Clear all tracked data
   */
  clear(): void {
    this.trackedExpressions.clear();
    this.changeTracker.clear();
  }

  /**
   * Get variable value from context
   */
  private getVariableValue(path: string, context: EvaluationContext): unknown {
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
}

/**
 * Create a new dependency manager instance
 */
export function createDependencyManager(): DependencyManager {
  return new DependencyManager();
}
