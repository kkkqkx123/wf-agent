/**
 * ToolContextStore - Tool Context Store
 * Specializes in managing the runtime context of tools, supporting the management of tools with different scopes.
 *
 * Core Responsibilities:
 * 1. Manages the runtime context of tools (tool ID, scope, metadata)
 * 2. Provides thread-isolated tool management
 * 3. Supports tools with different scopes (THREAD, LOCAL, GLOBAL)
 * 4. Offers atomic tool operations
 *
 * Design Principles:
 * - Only manages tool context; does not include business logic
 * - Thread isolation, with each thread having its own independent tool context
 * - Supports multiple levels of scopes (THREAD, LOCAL, GLOBAL)
 * - Atomic operations to ensure consistency of the tool context
 */

import type { ID } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";

/**
 * Tool Scope Types
 */
export type ToolScope = "GLOBAL" | "THREAD" | "LOCAL";

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
  /** Thread Scope Tools */
  threadTools: Map<string, ToolMetadata>;
  /** Local Scope Tool */
  localTools: Map<string, ToolMetadata>;
  /** Global Scope Tool */
  globalTools: Map<string, ToolMetadata>;
}

/**
 * ToolContextStore - Tool Context Store
 *
 * Responsibilities:
 * - Manages the runtime context of tools
 * - Provides thread-isolated tool management
 * - Supports tool management across different scopes
 * - Offers atomic tool operations
 *
 * Design Principles:
 * - Stateful design: Maintains the tool context
 * - Context management: Provides operations for creating, deleting, modifying, and querying tools
 * - Thread isolation: Each thread has its own independent tool context
 * - Scope support: Supports three scopes: THREAD, LOCAL, and GLOBAL
 */
export class ToolContextStore {
  /** Tool context mapping: threadId -> ToolContext */
  private contexts: Map<string, ToolContext> = new Map();

  /**
   * Obtain or create tool context
   */
  private getOrCreateContext(threadId: string): ToolContext {
    if (!this.contexts.has(threadId)) {
      this.contexts.set(threadId, {
        threadTools: new Map(),
        localTools: new Map(),
        globalTools: new Map(),
      });
    }
    return this.contexts.get(threadId)!;
  }

  /**
   * Add tools to the specified scope
   *
   * @param threadId Thread ID
   * @param workflowId Workflow ID
   * @param toolIds List of tool IDs
   * @param scope Tool scope
   * @param overwrite Whether to overwrite existing tools
   * @param descriptionTemplate Tool description template (optional)
   * @param customMetadata Custom metadata (optional)
   * @returns Number of tools successfully added
   */
  addTools(
    threadId: string,
    workflowId: ID,
    toolIds: string[],
    scope: ToolScope = "THREAD",
    overwrite: boolean = false,
    descriptionTemplate?: string,
    customMetadata?: Record<string, unknown>,
  ): number {
    const context = this.getOrCreateContext(threadId);
    let addedCount = 0;

    for (const toolId of toolIds) {
      const metadata: ToolMetadata = {
        toolId,
        descriptionTemplate,
        customMetadata,
        addedAt: now(),
      };

      let targetMap: Map<string, ToolMetadata>;

      switch (scope) {
        case "THREAD":
          targetMap = context.threadTools;
          break;
        case "LOCAL":
          targetMap = context.localTools;
          break;
        case "GLOBAL":
          targetMap = context.globalTools;
          break;
      }

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
   * @param threadId Thread ID
   * @param scope Tool scope (optional; if not specified, tools for all scopes will be returned)
   * @returns Collection of tool IDs
   */
  getTools(threadId: string, scope?: ToolScope): Set<string> {
    const context = this.contexts.get(threadId);
    if (!context) {
      return new Set();
    }

    if (scope) {
      switch (scope) {
        case "THREAD":
          return new Set(context.threadTools.keys());
        case "LOCAL":
          return new Set(context.localTools.keys());
        case "GLOBAL":
          return new Set(context.globalTools.keys());
      }
    }

    // Tools to return all scopes
    const allTools = new Set<string>();
    context.threadTools.forEach((_, toolId) => allTools.add(toolId));
    context.localTools.forEach((_, toolId) => allTools.add(toolId));
    context.globalTools.forEach((_, toolId) => allTools.add(toolId));

    return allTools;
  }

  /**
   * Retrieve tool metadata
   *
   * @param threadId Thread ID
   * @param toolId Tool ID
   * @param scope Tool scope (optional; if not specified, search all scopes)
   * @returns Tool metadata; returns undefined if not found
   */
  getToolMetadata(threadId: string, toolId: string, scope?: ToolScope): ToolMetadata | undefined {
    const context = this.contexts.get(threadId);
    if (!context) {
      return undefined;
    }

    if (scope) {
      switch (scope) {
        case "THREAD":
          return context.threadTools.get(toolId);
        case "LOCAL":
          return context.localTools.get(toolId);
        case "GLOBAL":
          return context.globalTools.get(toolId);
      }
    }

    // Search all scopes
    return (
      context.threadTools.get(toolId) ||
      context.localTools.get(toolId) ||
      context.globalTools.get(toolId)
    );
  }

  /**
   * Remove Tools
   *
   * @param threadId Thread ID
   * @param toolIds List of tool IDs
   * @param scope Tool scope (optional; if not specified, tools will be removed from all scopes)
   * @returns Number of tools successfully removed
   */
  removeTools(threadId: string, toolIds: string[], scope?: ToolScope): number {
    const context = this.contexts.get(threadId);
    if (!context) {
      return 0;
    }

    let removedCount = 0;

    if (scope) {
      let targetMap: Map<string, ToolMetadata>;
      switch (scope) {
        case "THREAD":
          targetMap = context.threadTools;
          break;
        case "LOCAL":
          targetMap = context.localTools;
          break;
        case "GLOBAL":
          targetMap = context.globalTools;
          break;
      }

      for (const toolId of toolIds) {
        if (targetMap.delete(toolId)) {
          removedCount++;
        }
      }
    } else {
      // Remove from all scopes
      for (const toolId of toolIds) {
        if (context.threadTools.delete(toolId)) {
          removedCount++;
        }
        if (context.localTools.delete(toolId)) {
          removedCount++;
        }
        if (context.globalTools.delete(toolId)) {
          removedCount++;
        }
      }
    }

    return removedCount;
  }

  /**
   * Tool for clearing a specified scope
   *
   * @param threadId Thread ID
   * @param scope Tool scope (optional; if not specified, all scopes will be cleared)
   * @returns Number of tools that were cleared
   */
  clearTools(threadId: string, scope?: ToolScope): number {
    const context = this.contexts.get(threadId);
    if (!context) {
      return 0;
    }

    let clearedCount = 0;

    if (scope) {
      switch (scope) {
        case "THREAD":
          clearedCount = context.threadTools.size;
          context.threadTools.clear();
          break;
        case "LOCAL":
          clearedCount = context.localTools.size;
          context.localTools.clear();
          break;
        case "GLOBAL":
          clearedCount = context.globalTools.size;
          context.globalTools.clear();
          break;
      }
    } else {
      clearedCount = context.threadTools.size + context.localTools.size + context.globalTools.size;
      context.threadTools.clear();
      context.localTools.clear();
      context.globalTools.clear();
    }

    return clearedCount;
  }

  /**
   * Check if the tool exists
   *
   * @param threadId Thread ID
   * @param toolId Tool ID
   * @param scope Tool scope (optional; if not specified, all scopes will be checked)
   * @returns Whether the tool exists
   */
  hasTool(threadId: string, toolId: string, scope?: ToolScope): boolean {
    const context = this.contexts.get(threadId);
    if (!context) {
      return false;
    }

    if (scope) {
      switch (scope) {
        case "THREAD":
          return context.threadTools.has(toolId);
        case "LOCAL":
          return context.localTools.has(toolId);
        case "GLOBAL":
          return context.globalTools.has(toolId);
      }
    }

    return (
      context.threadTools.has(toolId) ||
      context.localTools.has(toolId) ||
      context.globalTools.has(toolId)
    );
  }

  /**
   * Get a snapshot of the tool context
   *
   * @param threadId: Thread ID
   * @returns: Snapshot of the tool context
   */
  getSnapshot(threadId: string): ToolContext | undefined {
    const context = this.contexts.get(threadId);
    if (!context) {
      return undefined;
    }

    return {
      threadTools: new Map(context.threadTools),
      localTools: new Map(context.localTools),
      globalTools: new Map(context.globalTools),
    };
  }

  /**
   * Recovery tool context from a snapshot
   *
   * @param threadId: Thread ID
   * @param snapshot: Tool context snapshot
   */
  restoreSnapshot(threadId: string, snapshot: ToolContext): void {
    this.contexts.set(threadId, {
      threadTools: new Map(snapshot.threadTools),
      localTools: new Map(snapshot.localTools),
      globalTools: new Map(snapshot.globalTools),
    });
  }

  /**
   * Delete tool context
   *
   * @param threadId Thread ID
   */
  deleteContext(threadId: string): void {
    this.contexts.delete(threadId);
  }

  /**
   * Clear all tool contexts.
   */
  clearAll(): void {
    this.contexts.clear();
  }

  /**
   * Get all thread IDs
   *
   * @returns List of thread IDs
   */
  getAllThreadIds(): string[] {
    return Array.from(this.contexts.keys());
  }
}
