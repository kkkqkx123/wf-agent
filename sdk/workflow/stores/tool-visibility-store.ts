/**
 * ToolVisibilityStore - Tool Visibility Store
 * Manages the visibility status of tools
 */

import type { ToolScope } from "./tool-context-store.js";
import type { ToolVisibilityContext } from "../execution/types/tool-visibility.types.js";
import type { LifecycleCapable } from "../../core/types/lifecycle-capable.js";

/**
 * ToolVisibilityStore - Tool Visibility Store
 *
 * Responsibilities:
 * - Manages the visibility status of tools
 * - Provides thread-isolated visibility management
 * - Supports tool visibility in different scopes
 * - Offers functionality for taking and restoring state snapshots
 *
 * Design Principles:
 * - Stateful design: Maintains the visibility context
 * - State management: Provides operations for creating, reading, updating, and deleting visibility states
 * - Thread isolation: Each thread has its own independent visibility context
 * - Lifecycle management: Implements the LifecycleCapable interface
 */
export class ToolVisibilityStore implements LifecycleCapable {
  /** Tool Visibility Context Mapping: executionId -> ToolVisibilityContext */
  private contexts: Map<string, ToolVisibilityContext> = new Map();

  /**
   * Initialize visibility context
   * @param executionId Execution ID
   * @param availableTools List of available tool IDs
   * @param scope Scope
   * @param scopeId Scope ID
   */
  initializeContext(
    executionId: string,
    availableTools: string[],
    scope: ToolScope = "EXECUTION",
    scopeId: string = executionId,
  ): void {
    const context: ToolVisibilityContext = {
      currentScope: scope,
      scopeId,
      visibleTools: new Set(availableTools),
      declarationHistory: [],
      lastDeclarationIndex: -1,
      initializedAt: Date.now(),
    };

    this.contexts.set(executionId, context);
  }

  /**
   * Get visibility context
   * @param executionId: Execution ID
   * @returns: Tool visibility context; returns undefined if not present
   */
  getContext(executionId: string): ToolVisibilityContext | undefined {
    return this.contexts.get(executionId);
  }

  /**
   * Get the visible set of tools
   * @param executionId Execution ID
   * @returns The visible set of tools
   */
  getVisibleTools(executionId: string): Set<string> {
    const context = this.contexts.get(executionId);
    return context ? context.visibleTools : new Set();
  }

  /**
   * Update visibility
   * @param executionId Execution ID
   * @param newTools List of new tool IDs
   * @param scope Scope
   * @param scopeId Scope ID
   */
  updateVisibility(executionId: string, newTools: string[], scope: ToolScope, scopeId: string): void {
    const context = this.contexts.get(executionId);
    if (!context) {
      this.initializeContext(executionId, newTools, scope, scopeId);
      return;
    }

    context.currentScope = scope;
    context.scopeId = scopeId;
    context.visibleTools = new Set(newTools);
  }

  /**
   * Add tools to the visibility set
   * @param executionId Execution ID
   * @param toolIds List of tool IDs
   */
  addTools(executionId: string, toolIds: string[]): void {
    const context = this.contexts.get(executionId);
    if (!context) {
      return;
    }

    toolIds.forEach(id => context.visibleTools.add(id));
  }

  /**
   * Remove tools from the visibility set
   * @param executionId Execution ID
   * @param toolIds List of tool IDs
   */
  removeTools(executionId: string, toolIds: string[]): void {
    const context = this.contexts.get(executionId);
    if (!context) {
      return;
    }

    toolIds.forEach(id => context.visibleTools.delete(id));
  }

  /**
   * Check if the tool is visible
   * @param executionId Execution ID
   * @param toolId Tool ID
   * @returns Whether it is visible
   */
  isToolVisible(executionId: string, toolId: string): boolean {
    const context = this.contexts.get(executionId);
    if (!context) {
      return false;
    }
    return context.visibleTools.has(toolId);
  }

  /**
   * Remove visibility context
   * @param executionId Execution ID
   */
  deleteContext(executionId: string): void {
    this.contexts.delete(executionId);
  }

  /**
   * Clean up resources
   * Clear all visibility contexts
   */
  cleanup(): void {
    this.contexts.clear();
  }

  /**
   * Create a status snapshot
   * @returns Status snapshot
   */
  createSnapshot(): Map<string, ToolVisibilityContext> {
    const snapshot = new Map<string, ToolVisibilityContext>();
    for (const [executionId, context] of this.contexts.entries()) {
      snapshot.set(executionId, {
        ...context,
        visibleTools: new Set(context.visibleTools),
        declarationHistory: [...context.declarationHistory],
      });
    }
    return snapshot;
  }

  /**
   * Restore from snapshot state
   * @param snapshot: State snapshot
   */
  restoreFromSnapshot(snapshot: Map<string, ToolVisibilityContext>): void {
    this.contexts.clear();
    for (const [executionId, context] of snapshot.entries()) {
      this.contexts.set(executionId, {
        ...context,
        visibleTools: new Set(context.visibleTools),
        declarationHistory: [...context.declarationHistory],
      });
    }
  }

  /**
   * Get a snapshot of the visibility context
   * @param executionId: Execution ID
   * @returns: Snapshot of the visibility context
   */
  getSnapshot(executionId: string): ToolVisibilityContext | undefined {
    const context = this.contexts.get(executionId);
    if (!context) {
      return undefined;
    }

    return {
      ...context,
      visibleTools: new Set(context.visibleTools),
      declarationHistory: [...context.declarationHistory],
    };
  }

  /**
   * Restore visibility context from a snapshot
   * @param executionId: Execution ID
   * @param snapshot: Visibility context snapshot
   */
  restoreSnapshot(executionId: string, snapshot: ToolVisibilityContext): void {
    this.contexts.set(executionId, {
      ...snapshot,
      visibleTools: new Set(snapshot.visibleTools),
      declarationHistory: [...snapshot.declarationHistory],
    });
  }

  /**
   * Get all execution IDs
   * @returns List of all execution IDs
   */
  getAllExecutionIds(): string[] {
    return Array.from(this.contexts.keys());
  }

  /**
   * Get the number of contexts
   * @returns The number of contexts
   */
  getContextCount(): number {
    return this.contexts.size;
  }
}
