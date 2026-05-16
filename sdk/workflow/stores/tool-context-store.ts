/**
 * ToolContextStore - Tool Context Store
 * Specializes in managing the runtime context of tools, supporting execution-isolated tool management.
 *
 * Core Responsibilities:
 * 1. Manages the runtime context of tools (tool ID, scope, metadata)
 * 2. Provides execution-isolated tool management
 * 3. Supports two scopes: EXECUTION (default) and LOCAL (for subgraph/isolated contexts)
 * 4. Offers atomic tool operations
 *
 * Design Principles:
 * - Only manages tool context; does not include business logic
 * - Execution isolation, with each workflow execution having its own independent tool context
 * - Simple two-level scope hierarchy for clarity and maintainability
 * - Atomic operations to ensure consistency of the tool context
 */

import { now } from "@wf-agent/common-utils";

/**
 * Tool Scope Types
 * 
 * Scope Hierarchy (from most specific to most general):
 * - LOCAL: Tools available only in the current local/subgraph context
 * - EXECUTION: Tools available in the current workflow execution instance (default)
 * 
 * Note: Global and workflow-level tools should be configured via ToolRegistry and availableTools.initial,
 * not managed dynamically at runtime.
 */
export type ToolScope = "EXECUTION" | "LOCAL";

/**
 * Tool metadata
 */
export interface ToolMetadata {
  /** Tool ID */
  toolId: string;
  /** Tool Description Template (optional) */
  descriptionTemplate?: string;
  /** Custom Metadata */
  customMetadata?: Record<string, unknown>;
  /** Add a timestamp */
  addedAt: number;
}

/**
 * Tool Context Structure
 */
export interface ToolContext {
  /** Local Scope Tools (tools specific to subgraph/local context) */
  localTools: Map<string, ToolMetadata>;
  /** Execution Scope Tools (tools specific to this workflow execution instance) */
  executionTools: Map<string, ToolMetadata>;
}

/**
 * ToolContextStore - Tool Context Store
 *
 * Responsibilities:
 * - Manages the runtime context of tools
 * - Provides execution-isolated tool management
 * - Supports two scopes: EXECUTION (default) and LOCAL
 * - Offers atomic tool operations
 *
 * Design Principles:
 * - Stateful design: Maintains the tool context
 * - Context management: Provides operations for creating, deleting, modifying, and querying tools
 * - Execution isolation: Each workflow execution has its own independent tool context
 * - Simplified scope model: Only EXECUTION and LOCAL scopes for clarity
 */
export class ToolContextStore {
  /** Execution-level tool context mapping: executionId -> ToolContext */
  private executionContexts: Map<string, ToolContext> = new Map();

  /**
   * Obtain or create execution-level tool context
   */
  private getOrCreateExecutionContext(executionId: string): ToolContext {
    if (!this.executionContexts.has(executionId)) {
      this.executionContexts.set(executionId, {
        localTools: new Map(),
        executionTools: new Map(),
      });
    }
    return this.executionContexts.get(executionId)!;
  }

  /**
   * Add tools to the specified scope
   *
   * @param executionId Execution ID
   * @param toolIds List of tool IDs
   * @param scope Tool scope (defaults to EXECUTION)
   * @param overwrite Whether to overwrite existing tools
   * @param descriptionTemplate Tool description template (optional)
   * @param customMetadata Custom metadata (optional)
   * @returns Number of tools successfully added
   */
  addTools(
    executionId: string,
    toolIds: string[],
    scope: ToolScope = "EXECUTION",
    overwrite: boolean = false,
    descriptionTemplate?: string,
    customMetadata?: Record<string, unknown>,
  ): number {
    const context = this.getOrCreateExecutionContext(executionId);
    let addedCount = 0;

    for (const toolId of toolIds) {
      const metadata: ToolMetadata = {
        toolId,
        descriptionTemplate,
        customMetadata,
        addedAt: now(),
      };

      const targetMap = scope === "LOCAL" ? context.localTools : context.executionTools;

      // Check if it already exists.
      if (targetMap.has(toolId)) {
        if (overwrite) {
          targetMap.set(toolId, metadata);
          addedCount++;
        }
        // If not overridden, skip it.
      } else {
        targetMap.set(toolId, metadata);
        addedCount++;
      }
    }

    return addedCount;
  }

  /**
   * Get a collection of tools for the specified scope
   *
   * @param executionId Execution ID
   * @param scope Tool scope (optional; if not specified, tools for all scopes will be returned)
   * @returns Collection of tool IDs
   */
  getTools(executionId: string, scope?: ToolScope): Set<string> {
    const context = this.executionContexts.get(executionId);
    if (!context) {
      return new Set();
    }

    if (scope) {
      return scope === "LOCAL" 
        ? new Set(context.localTools.keys())
        : new Set(context.executionTools.keys());
    }

    // Return tools from all scopes
    const allTools = new Set<string>();
    context.executionTools.forEach((_, toolId) => allTools.add(toolId));
    context.localTools.forEach((_, toolId) => allTools.add(toolId));

    return allTools;
  }

  /**
   * Retrieve tool metadata
   *
   * @param executionId Execution ID
   * @param toolId Tool ID
   * @param scope Tool scope (optional; if not specified, search all scopes)
   * @returns Tool metadata; returns undefined if not found
   */
  getToolMetadata(
    executionId: string,
    toolId: string,
    scope?: ToolScope,
  ): ToolMetadata | undefined {
    const context = this.executionContexts.get(executionId);
    if (!context) {
      return undefined;
    }

    if (scope) {
      return scope === "LOCAL"
        ? context.localTools.get(toolId)
        : context.executionTools.get(toolId);
    }

    // Search all scopes (priority: LOCAL > EXECUTION)
    return context.localTools.get(toolId) || context.executionTools.get(toolId);
  }

  /**
   * Remove Tools
   *
   * @param executionId Execution ID
   * @param toolIds List of tool IDs
   * @param scope Tool scope (optional; if not specified, tools will be removed from all scopes)
   * @returns Number of tools successfully removed
   */
  removeTools(executionId: string, toolIds: string[], scope?: ToolScope): number {
    const context = this.executionContexts.get(executionId);
    if (!context) {
      return 0;
    }

    let removedCount = 0;

    if (scope) {
      const targetMap = scope === "LOCAL" ? context.localTools : context.executionTools;
      for (const toolId of toolIds) {
        if (targetMap.delete(toolId)) {
          removedCount++;
        }
      }
    } else {
      // Remove from all scopes
      for (const toolId of toolIds) {
        if (context.executionTools.delete(toolId)) {
          removedCount++;
        }
        if (context.localTools.delete(toolId)) {
          removedCount++;
        }
      }
    }

    return removedCount;
  }

  /**
   * Clear tools from a specified scope
   *
   * @param executionId Execution ID
   * @param scope Tool scope (optional; if not specified, all scopes will be cleared)
   * @returns Number of tools that were cleared
   */
  clearTools(executionId: string, scope?: ToolScope): number {
    const context = this.executionContexts.get(executionId);
    if (!context) {
      return 0;
    }

    let clearedCount = 0;

    if (scope) {
      if (scope === "LOCAL") {
        clearedCount = context.localTools.size;
        context.localTools.clear();
      } else {
        clearedCount = context.executionTools.size;
        context.executionTools.clear();
      }
    } else {
      // Clear all scopes
      clearedCount = context.executionTools.size + context.localTools.size;
      context.executionTools.clear();
      context.localTools.clear();
    }

    return clearedCount;
  }

  /**
   * Check if the tool exists
   *
   * @param executionId Execution ID
   * @param toolId Tool ID
   * @param scope Tool scope (optional; if not specified, all scopes will be checked)
   * @returns Whether the tool exists
   */
  hasTool(executionId: string, toolId: string, scope?: ToolScope): boolean {
    const context = this.executionContexts.get(executionId);
    if (!context) {
      return false;
    }

    if (scope) {
      return scope === "LOCAL"
        ? context.localTools.has(toolId)
        : context.executionTools.has(toolId);
    }

    // Check all scopes
    return context.localTools.has(toolId) || context.executionTools.has(toolId);
  }

  /**
   * Get a snapshot of the tool context
   *
   * @param executionId: Execution ID
   * @returns: Snapshot of the tool context (only execution-level tools)
   */
  getSnapshot(executionId: string): ToolContext | undefined {
    const context = this.executionContexts.get(executionId);
    if (!context) {
      return undefined;
    }

    return {
      localTools: new Map(context.localTools),
      executionTools: new Map(context.executionTools),
    };
  }

  /**
   * Restore tool context from a snapshot
   *
   * @param executionId: Execution ID
   * @param snapshot: Tool context snapshot (only execution-level tools)
   */
  restoreSnapshot(executionId: string, snapshot: ToolContext): void {
    this.executionContexts.set(executionId, {
      localTools: new Map(snapshot.localTools),
      executionTools: new Map(snapshot.executionTools),
    });
  }

  /**
   * Delete execution-level tool context
   *
   * @param executionId Execution ID
   */
  deleteContext(executionId: string): void {
    this.executionContexts.delete(executionId);
  }

  /**
   * Clear all tool contexts.
   */
  clearAll(): void {
    this.executionContexts.clear();
  }

  /**
   * Get all execution IDs with tool contexts
   *
   * @returns List of execution IDs
   */
  getAllExecutionIds(): string[] {
    return Array.from(this.executionContexts.keys());
  }
}
