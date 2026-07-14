/**
 * VariableResourceAPI - Variable Resource Management API
 * inherits from QueryableResourceAPI, providing read-only operations
 */

import { now } from "@wf-agent/common-utils";
import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { WorkflowExecutionRegistry } from "../../../workflow/registry/workflow-execution-registry.js";
import { NotFoundError, WorkflowExecutionNotFoundError, SDKError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { ExecutionResult } from "../../shared/types/execution-result.js";
import { success, failure } from "../../shared/types/execution-result.js";

/**
 * Variable Filter
 */
export interface VariableFilter {
  /** Variable names (fuzzy matching is supported) */
  name?: string;
  /** Variable Scope */
  scope?: "global" | "workflowExecution" | "subgraph" | "loop";
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
export class VariableResourceAPI extends QueryableResourceAPI<unknown, string, VariableFilter> {
  private registry: WorkflowExecutionRegistry;

  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getWorkflowExecutionRegistry();
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
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);

    // Use VariableManager to get variable (respects scope priority)
    const value = executionEntity.variableStateManager.getVariable(variableName);
    return value ?? null;
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
   * Get all variable values of the workflow execution
   * @param executionId: Execution ID
   * @returns: Record of variable values
   */
  async getWorkflowExecutionVariables(executionId: string): Promise<Record<string, unknown>> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    // Use VariableManager to get all variables (respects scope priority)
    return executionEntity.variableStateManager.getAllVariables();
  }

  /**
   * Get the value of a specified variable for a workflow execution
   * @param executionId: Execution ID
   * @param name: Variable name
   * @returns: Variable value
   */
  async getWorkflowExecutionVariable(executionId: string, name: string): Promise<unknown> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);

    const value = executionEntity.variableStateManager.getVariable(name);
    if (value === undefined) {
      throw new NotFoundError(`Variable not found: ${name}`, "Variable", name);
    }

    return value;
  }

  /**
   * Check if the specified variable exists in the workflow execution.
   * @param executionId: Execution ID
   * @param name: Variable name
   * @returns: Whether the variable exists
   */
  async hasWorkflowExecutionVariable(executionId: string, name: string): Promise<boolean> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    return executionEntity.variableStateManager.hasVariable(name);
  }

  /**
   * Get variable definitions for a workflow execution
   * @param executionId Execution ID
   * @returns Record of variable definitions (name -> definition)
   */
  async getWorkflowExecutionVariableDefinitions(
    executionId: string,
  ): Promise<Record<string, unknown>> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    const definitions = executionEntity.variableStateManager.getAllVariableDefinitions();

    // Convert array to record format
    const result: Record<string, unknown> = {};
    for (const def of definitions) {
      result[def.name] = def;
    }
    return result;
  }

  /**
   * Get variable statistics for all executions
   * @returns Statistical information
   */
  async getVariableStatistics(): Promise<{
    totalExecutions: number;
    totalVariables: number;
    byExecution: Record<string, number>;
    byType: Record<string, number>;
  }> {
    const executionEntities = this.registry.getAll();
    const stats = {
      totalExecutions: executionEntities.length,
      totalVariables: 0,
      byExecution: {} as Record<string, number>,
      byType: {} as Record<string, number>,
    };

    for (const executionEntity of executionEntities) {
      const executionId = executionEntity.id;
      const manager = executionEntity.variableStateManager;
      const variables = manager.getAllVariables();

      stats.byExecution[executionId] = Object.keys(variables).length;
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
   * Get variable scopes information (deprecated - scope mechanism removed)
   * This method is kept for backward compatibility but returns simplified data
   * @param executionId Execution ID
   * @returns Scope information (all variables in execution field)
   */
  async getVariableScopes(executionId: string): Promise<{
    execution: Record<string, unknown>;
    global: Record<string, unknown>;
    subgraph: Record<string, unknown>; // Always empty - deprecated
    loop: Record<string, unknown>; // Always empty - deprecated
    currentScopeDepth: number;
    hasActiveTemporaryScope: boolean;
  }> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    const manager = executionEntity.variableStateManager;

    // Get all variables (flat structure)
    const allVars = manager.getAllVariables();

    return {
      execution: allVars,
      global: {}, // Deprecated - no longer used
      subgraph: {}, // Always empty - internal use only
      loop: {}, // Always empty - internal use only
      currentScopeDepth: 0, // Deprecated - no scope stack
      hasActiveTemporaryScope: false, // Deprecated - no scope stack
    };
  }

  /**
   * Search for variables
   * @param executionId Execution ID
   * @param query Search keyword
   * @returns Array of matching variable names
   */
  async searchVariables(executionId: string, query: string): Promise<string[]> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    const variables = executionEntity.variableStateManager.getAllVariables();

    return Object.keys(variables).filter(name => name.toLowerCase().includes(query.toLowerCase()));
  }

  /**
   * Export workflow execution variables
   * @param executionId Execution ID
   * @returns JSON string
   */
  async exportWorkflowExecutionVariables(executionId: string): Promise<string> {
    const variables = await this.getWorkflowExecutionVariables(executionId);
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
    const currentValue = await this.getWorkflowExecutionVariable(executionId, variableName);
    return [
      {
        timestamp: now(),
        value: currentValue,
        source: "current",
      },
    ];
  }

  /**
   * Set variable value for an execution
   * This is a convenience method that encapsulates direct registry access
   *
   * @param executionId Execution ID
   * @param variableName Variable name
   * @param value Variable value
   * @returns Execution result
   */
  async setVariable(
    executionId: string,
    variableName: string,
    value: unknown,
  ): Promise<ExecutionResult<void>> {
    try {
      const executionContext = this.registry.get(executionId);
      if (!executionContext) {
        return failure(
          new WorkflowExecutionNotFoundError(
            `Workflow execution not found: ${executionId}`,
            executionId,
          ),
          0,
        );
      }

      await executionContext.setVariable(variableName, value);
      return success(undefined, 0);
    } catch (error) {
      const sdkError =
        error instanceof SDKError
          ? error
          : error instanceof Error
            ? new SDKError(error.message, "error", undefined, error)
            : new SDKError(String(error), "error");
      return failure(sdkError, 0);
    }
  }

  /**
   * Delete variable for an execution
   * This is a convenience method that encapsulates direct registry access
   *
   * @param executionId Execution ID
   * @param variableName Variable name
   * @returns Execution result
   */
  async deleteVariable(executionId: string, variableName: string): Promise<ExecutionResult<void>> {
    try {
      const executionContext = this.registry.get(executionId);
      if (!executionContext) {
        return failure(
          new WorkflowExecutionNotFoundError(
            `Workflow execution not found: ${executionId}`,
            executionId,
          ),
          0,
        );
      }

      await executionContext.deleteVariable(variableName);
      return success(undefined, 0);
    } catch (error) {
      const sdkError =
        error instanceof SDKError
          ? error
          : error instanceof Error
            ? new SDKError(error.message, "error", undefined, error)
            : new SDKError(String(error), "error");
      return failure(sdkError, 0);
    }
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
      throw new Error(
        `Invalid variable ID format: ${id}. Expected format: executionId:variableName`,
      );
    }
    return [parts[0]!, parts[1]!];
  }

  /**
   * Obtain an execution entity
   */
  private async getWorkflowExecutionEntity(executionId: string) {
    const executionEntity = this.registry.get(executionId);
    if (!executionEntity) {
      throw new WorkflowExecutionNotFoundError(
        `Workflow execution not found: ${executionId}`,
        executionId,
      );
    }
    return executionEntity;
  }

  /**
   * Obtain the underlying WorkflowExecutionRegistry instance
   * @returns WorkflowExecutionRegistry instance
   */
  getRegistry(): WorkflowExecutionRegistry {
    return this.registry;
  }
}
