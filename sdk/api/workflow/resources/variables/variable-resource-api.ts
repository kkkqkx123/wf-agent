/**
 * VariableResourceAPI - Variable Resource Management API
 * inherits from ReadonlyResourceAPI, providing read-only operations
 */

import { now } from "@wf-agent/common-utils";
import { ReadonlyResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { WorkflowExecutionRegistry } from "../../../../workflow/stores/workflow-execution-registry.js";
import type { WorkflowExecution } from "@wf-agent/types";
import { NotFoundError, WorkflowExecutionNotFoundError } from "@wf-agent/types";
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
  /** Execution ID */
  executionId?: string;
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
  private registry: WorkflowExecutionRegistry;

  constructor() {
    super();
    const container = getContainer();
    this.registry = container.get(Identifiers.WorkflowExecutionRegistry) as WorkflowExecutionRegistry;
  }

  // ============================================================================
  // Implement the abstract method
  // ============================================================================

  /**
   * Get the value of a single variable
   * @param id The variable name (format: executionId:variableName)
   * @returns The variable value; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<unknown | null> {
    const [executionId, variableName] = this.parseVariableId(id);
    const thread = await this.getWorkflowExecution(executionId);

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
   * @param executionId: Execution ID
   * @returns: Record of variable values
   */
  async getThreadVariables(executionId: string): Promise<Record<string, unknown>> {
    const executionEntity = await this.getWorkflowExecution(executionId);
    return { ...executionEntity.variableScopes.thread };
  }

  /**
   * Get the value of a specified variable for a thread
   * @param executionId: Execution ID
   * @param name: Variable name
   * @returns: Variable value
   */
  async getThreadVariable(executionId: string, name: string): Promise<unknown> {
    const executionEntity = await this.getWorkflowExecution(executionId);

    if (!(name in executionEntity.variableScopes.thread)) {
      throw new NotFoundError(`Variable not found: ${name}`, "Variable", name);
    }

    return executionEntity.variableScopes.thread[name];
  }

  /**
   * Check if the specified variable exists in the thread.
   * @param executionId: Execution ID
   * @param name: Variable name
   * @returns: Whether the variable exists
   */
  async hasThreadVariable(executionId: string, name: string): Promise<boolean> {
    const thread = await this.getWorkflowExecution(executionId);
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

    for (const executionEntity of threadContexts) {
      const executionId = executionEntity.id;
      const thread = executionEntity.getExecution();
      const variables = thread.variableScopes.thread;

      stats.byThread[executionId] = Object.keys(variables).length;
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
   * @param executionId Execution ID
   * @returns Scope information
   */
  async getVariableScopes(executionId: string): Promise<{
    thread: Record<string, unknown>;
    global: Record<string, unknown>;
    local: Record<string, unknown>;
    loop: Record<string, unknown>;
  }> {
    const thread = await this.getWorkflowExecution(executionId);
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
   * @param executionId Execution ID
   * @param query Search keyword
   * @returns Array of matching variable names
   */
  async searchVariables(executionId: string, query: string): Promise<string[]> {
    const thread = await this.getWorkflowExecution(executionId);
    const variables = thread.variableScopes.thread;

    return Object.keys(variables).filter(name => name.toLowerCase().includes(query.toLowerCase()));
  }

  /**
   * Export thread variable
   * @param executionId Execution ID
   * @returns JSON string
   */
  async exportThreadVariables(executionId: string): Promise<string> {
    const variables = await this.getThreadVariables(executionId);
    return JSON.stringify(variables, null, 2);
  }

  /**
   * Get variable change history
   * @param executionId Execution ID
   * @param variableName Variable name
   * @returns Array of change history (simplified implementation)
   */
  async getVariableHistory(
    executionId: string,
    variableName: string,
  ): Promise<
    Array<{
      timestamp: number;
      value: unknown;
      source: string;
    }>
  > {
    // Simplify the implementation; in real projects, you can obtain the necessary data from the event system.
    const currentValue = await this.getThreadVariable(executionId, variableName);
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
   * @param id Variable ID (format: executionId:variableName)
   * @returns [executionId, variableName]
   */
  private parseVariableId(id: string): [string, string] {
    const parts = id.split(":");
    if (parts.length !== 2) {
      throw new Error(`Invalid variable ID format: ${id}. Expected format: executionId:variableName`);
    }
    return [parts[0]!, parts[1]!];
  }

  /**
   * Obtain a thread instance
   */
  private async getWorkflowExecution(executionId: string): Promise<WorkflowExecution> {
    const threadContext = this.registry.get(executionId);
    if (!threadContext) {
      throw new WorkflowExecutionNotFoundError(`Workflow execution not found: ${executionId}`, executionId);
    }
    return threadContext.getExecution();
  }

  /**
   * Obtain the underlying WorkflowExecutionRegistry instance
   * @returns WorkflowExecutionRegistry instance
   */
  getRegistry(): WorkflowExecutionRegistry {
    return this.registry;
  }
}
