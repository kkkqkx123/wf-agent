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
  /** Tool Visibility Context Mapping: threadId -> ToolVisibilityContext */
  private contexts: Map<string, ToolVisibilityContext> = new Map();

  /**
   * Initialize visibility context
   * @param threadId Thread ID
   * @param availableTools List of available tool IDs
   * @param scope Scope
   * @param scopeId Scope ID
   */
  initializeContext(
    threadId: string,
    availableTools: string[],
    scope: ToolScope = "THREAD",
    scopeId: string = threadId,
  ): void {
    const context: ToolVisibilityContext = {
      currentScope: scope,
      scopeId,
      visibleTools: new Set(availableTools),
      declarationHistory: [],
      lastDeclarationIndex: -1,
      initializedAt: Date.now(),
    };

    this.contexts.set(threadId, context);
  }

  /**
   * Get visibility context
   * @param threadId: Thread ID
   * @returns: Tool visibility context; returns undefined if not present
   */
  getContext(threadId: string): ToolVisibilityContext | undefined {
    return this.contexts.get(threadId);
  }

  /**
   * Get the visible set of tools
   * @param threadId Thread ID
   * @returns The visible set of tools
   */
  getVisibleTools(threadId: string): Set<string> {
    const context = this.contexts.get(threadId);
    return context ? context.visibleTools : new Set();
  }

  /**
   * Update visibility
   * @param threadId Thread ID
   * @param newTools List of new tool IDs
   * @param scope Scope
   * @param scopeId Scope ID
   */
  updateVisibility(threadId: string, newTools: string[], scope: ToolScope, scopeId: string): void {
    const context = this.contexts.get(threadId);
    if (!context) {
      this.initializeContext(threadId, newTools, scope, scopeId);
      return;
    }

    context.currentScope = scope;
    context.scopeId = scopeId;
    context.visibleTools = new Set(newTools);
  }

  /**
   * Add tools to the visibility set
   * @param threadId Thread ID
   * @param toolIds List of tool IDs
   */
  addTools(threadId: string, toolIds: string[]): void {
    const context = this.contexts.get(threadId);
    if (!context) {
      return;
    }

    toolIds.forEach(id => context.visibleTools.add(id));
  }

  /**
   * Remove tools from the visibility set
   * @param threadId Thread ID
   * @param toolIds List of tool IDs
   */
  removeTools(threadId: string, toolIds: string[]): void {
    const context = this.contexts.get(threadId);
    if (!context) {
      return;
    }

    toolIds.forEach(id => context.visibleTools.delete(id));
  }

  /**
   * Check if the tool is visible
   * @param threadId Thread ID
   * @param toolId Tool ID
   * @returns Whether it is visible
   */
  isToolVisible(threadId: string, toolId: string): boolean {
    const context = this.contexts.get(threadId);
    if (!context) {
      return false;
    }
    return context.visibleTools.has(toolId);
  }

  /**
   * Remove visibility context
   * @param threadId Thread ID
   */
  deleteContext(threadId: string): void {
    this.contexts.delete(threadId);
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
    for (const [threadId, context] of this.contexts.entries()) {
      snapshot.set(threadId, {
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
    for (const [threadId, context] of snapshot.entries()) {
      this.contexts.set(threadId, {
        ...context,
        visibleTools: new Set(context.visibleTools),
        declarationHistory: [...context.declarationHistory],
      });
    }
  }

  /**
   * Get a snapshot of the visibility context
   * @param threadId: Thread ID
   * @returns: Snapshot of the visibility context
   */
  getSnapshot(threadId: string): ToolVisibilityContext | undefined {
    const context = this.contexts.get(threadId);
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
   * @param threadId: Thread ID
   * @param snapshot: Visibility context snapshot
   */
  restoreSnapshot(threadId: string, snapshot: ToolVisibilityContext): void {
    this.contexts.set(threadId, {
      ...snapshot,
      visibleTools: new Set(snapshot.visibleTools),
      declarationHistory: [...snapshot.declarationHistory],
    });
  }

  /**
   * Get all thread IDs
   * @returns List of all thread IDs
   */
  getAllThreadIds(): string[] {
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
