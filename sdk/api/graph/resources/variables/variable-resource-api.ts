/**
 * VariableResourceAPI - Variable Resource Management API
 * inherits from ReadonlyResourceAPI, providing read-only operations
 */

import { now } from "@wf-agent/common-utils";
import { ReadonlyResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { ThreadRegistry } from "../../../../graph/stores/thread-registry.js";
import type { Thread } from "@wf-agent/types";
import { NotFoundError, ThreadContextNotFoundError } from "@wf-agent/types";
import { getContainer } from "../../../../core/di/index.js";
import * as Identifiers from "../../../../core/di/service-identifiers.js";

/**
 * Variable Filter
 */
export interface VariableFilter {
  /** Variable names (fuzzy matching is supported) */
  name?: string;
  /** Variable Scope */
  scope?: "global" | "thread" | "subgraph" | "loop";
  /** Variable Types */
  type?: string;
  /** Thread ID */
  threadId?: string;
  /** Workflow ID */
  workflowId?: string;
}

/**
 * Variable update options
 */
export interface VariableUpdateOptions {
  /** Whether to overwrite an existing variable */
  overwrite?: boolean;
  /** Whether to validate the variable value */
  validate?: boolean;
  /** Whether to trigger the event */
  triggerEvent?: boolean;
}

/**
 * Variable definition information
 */
export interface VariableDefinition {
  /** Variable names */
  name: string;
  /** Variable Types */
  type: string;
  /** Variable Description */
  description?: string;
  /** Default value */
  defaultValue?: unknown;
  /** Is it mandatory? */
  required?: boolean;
}

/**
 * VariableResourceAPI - Variable Resource Management API
 */
export class VariableResourceAPI extends ReadonlyResourceAPI<unknown, string, VariableFilter> {
  private registry: ThreadRegistry;

  constructor() {
    super();
    const container = getContainer();
    this.registry = container.get(Identifiers.ThreadRegistry) as ThreadRegistry;
  }

  // ============================================================================
  // Implement the abstract method
  // ============================================================================

  /**
   * Get the value of a single variable
   * @param id The variable name (format: threadId:variableName)
   * @returns The variable value; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<unknown | null> {
    const [threadId, variableName] = this.parseVariableId(id);
    const thread = await this.getThread(threadId);

    if (!(variableName in thread.variableScopes.thread)) {
      return null;
    }

    return thread.variableScopes.thread[variableName];
  }

  /**
   * Get all variable values
   * @returns Record of variable values
   */
  protected async getAllResources(): Promise<unknown[]> {
    // The variable is not suitable to return an array, so an empty array is returned instead. A specific method is used to obtain the variable.
    return [];
  }

  // ============================================================================
  // Variable-specific methods
  // ============================================================================

  /**
   * Get all variable values of the thread
   * @param threadId: Thread ID
   * @returns: Record of variable values
   */
  async getThreadVariables(threadId: string): Promise<Record<string, unknown>> {
    const threadEntity = await this.getThread(threadId);
    return { ...threadEntity.variableScopes.thread };
  }

  /**
   * Get the value of a specified variable for a thread
   * @param threadId: Thread ID
   * @param name: Variable name
   * @returns: Variable value
   */
  async getThreadVariable(threadId: string, name: string): Promise<unknown> {
    const threadEntity = await this.getThread(threadId);

    if (!(name in threadEntity.variableScopes.thread)) {
      throw new NotFoundError(`Variable not found: ${name}`, "Variable", name);
    }

    return threadEntity.variableScopes.thread[name];
  }

  /**
   * Check if the specified variable exists in the thread.
   * @param threadId: Thread ID
   * @param name: Variable name
   * @returns: Whether the variable exists
   */
  async hasThreadVariable(threadId: string, name: string): Promise<boolean> {
    const thread = await this.getThread(threadId);
    return name in thread.variableScopes.thread;
  }

  /**
   * Get variable definitions for a thread
   * @returns: Record of variable definitions
   */
  async getThreadVariableDefinitions(): Promise<Record<string, unknown>> {
    // The variable definition information needs to be obtained from another source; therefore, an empty object is returned here.
    return {};
  }

  /**
   * Get variable statistics for all threads
   * @returns Statistical information
   */
  async getVariableStatistics(): Promise<{
    totalThreads: number;
    totalVariables: number;
    byThread: Record<string, number>;
    byType: Record<string, number>;
  }> {
    const threadContexts = this.registry.getAll();
    const stats = {
      totalThreads: threadContexts.length,
      totalVariables: 0,
      byThread: {} as Record<string, number>,
      byType: {} as Record<string, number>,
    };

    for (const threadEntity of threadContexts) {
      const threadId = threadEntity.id;
      const thread = threadEntity.getThread();
      const variables = thread.variableScopes.thread;

      stats.byThread[threadId] = Object.keys(variables).length;
      stats.totalVariables += Object.keys(variables).length;

      // Statistical variable type determination (simplified implementation)
      for (const [, value] of Object.entries(variables)) {
        const type = typeof value;
        stats.byType[type] = (stats.byType[type] || 0) + 1;
      }
    }

    return stats;
  }

  /**
   * Obtain variable scope information
   * @param threadId Thread ID
   * @returns Scope information
   */
  async getVariableScopes(threadId: string): Promise<{
    thread: Record<string, unknown>;
    global: Record<string, unknown>;
    local: Record<string, unknown>;
    loop: Record<string, unknown>;
  }> {
    const thread = await this.getThread(threadId);
    return {
      thread: { ...thread.variableScopes.thread },
      global: { ...thread.variableScopes.global },
      local:
        thread.variableScopes.local.length > 0
          ? { ...thread.variableScopes.local[thread.variableScopes.local.length - 1] }
          : {},
      loop:
        thread.variableScopes.loop.length > 0
          ? { ...thread.variableScopes.loop[thread.variableScopes.loop.length - 1] }
          : {},
    };
  }

  /**
   * Search for variables
   * @param threadId Thread ID
   * @param query Search keyword
   * @returns Array of matching variable names
   */
  async searchVariables(threadId: string, query: string): Promise<string[]> {
    const thread = await this.getThread(threadId);
    const variables = thread.variableScopes.thread;

    return Object.keys(variables).filter(name => name.toLowerCase().includes(query.toLowerCase()));
  }

  /**
   * Export thread variable
   * @param threadId Thread ID
   * @returns JSON string
   */
  async exportThreadVariables(threadId: string): Promise<string> {
    const variables = await this.getThreadVariables(threadId);
    return JSON.stringify(variables, null, 2);
  }

  /**
   * Get variable change history
   * @param threadId Thread ID
   * @param variableName Variable name
   * @returns Array of change history (simplified implementation)
   */
  async getVariableHistory(
    threadId: string,
    variableName: string,
  ): Promise<
    Array<{
      timestamp: number;
      value: unknown;
      source: string;
    }>
  > {
    // Simplify the implementation; in real projects, you can obtain the necessary data from the event system.
    const currentValue = await this.getThreadVariable(threadId, variableName);
    return [
      {
        timestamp: now(),
        value: currentValue,
        source: "current",
      },
    ];
  }

  // ============================================================================
  // Auxiliary method
  // ============================================================================

  /**
   * Parse variable ID
   * @param id Variable ID (format: threadId:variableName)
   * @returns [threadId, variableName]
   */
  private parseVariableId(id: string): [string, string] {
    const parts = id.split(":");
    if (parts.length !== 2) {
      throw new Error(`Invalid variable ID format: ${id}. Expected format: threadId:variableName`);
    }
    return [parts[0]!, parts[1]!];
  }

  /**
   * Obtain a thread instance
   */
  private async getThread(threadId: string): Promise<Thread> {
    const threadContext = this.registry.get(threadId);
    if (!threadContext) {
      throw new ThreadContextNotFoundError(`Thread not found: ${threadId}`, threadId);
    }
    return threadContext.getThread();
  }

  /**
   * Obtain the underlying ThreadRegistry instance
   * @returns ThreadRegistry instance
   */
  getRegistry(): ThreadRegistry {
    return this.registry;
  }
}
