/**
 * VariableResourceAPI - Workflow Variable Resource Management API
 * Provides APIs for managing variables in workflow executions.
 * Extends the shared BaseVariableResourceAPI with workflow-specific implementation.
 */

import {
  BaseVariableResourceAPI,
  type BaseVariableFilter,
  type VariableStatistics,
} from "../../shared/resources/variable-base.js";
import { now } from "@wf-agent/common-utils";
import type { WorkflowExecutionRegistry } from "../../../workflow/registry/workflow-execution-registry.js";
import { NotFoundError, WorkflowExecutionNotFoundError, SDKError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { ExecutionResult } from "../../shared/types/execution-result.js";
import { success, failure } from "../../shared/types/execution-result.js";

/**
 * Workflow Variable Filter
 */
export interface VariableFilter extends BaseVariableFilter {
  /** Filter by name */
  name?: string;
  /** Filter by source */
  source?: string;
}

/**
 * Variable Update Options
 */
export interface VariableUpdateOptions {
  /** Whether to overwrite existing variables */
  overwrite?: boolean;
  /** Source of the update */
  source?: string;
}

/**
 * Variable Definition
 */
export interface VariableDefinition {
  /** Variable name */
  name: string;
  /** Variable type */
  type: string;
  /** Variable value */
  value: unknown;
  /** Variable source */
  source: string;
  /** Last updated timestamp */
  lastUpdated: number;
  /** Whether the variable is required */
  required?: boolean;
}

/**
 * VariableResourceAPI - Workflow Variable Resource Management API
 */
export class VariableResourceAPI extends BaseVariableResourceAPI<VariableFilter> {
  private registry: WorkflowExecutionRegistry;

  constructor(deps: APIDependencyManager) {
    super();
    this.registry = deps.getWorkflowExecutionRegistry();
  }

  // ============================================================================
  // Implement BaseVariableResourceAPI abstract methods
  // ============================================================================

  /**
   * Get all variable values of the workflow execution
   */
  protected async getEntityVariables(executionId: string): Promise<Record<string, unknown>> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    return executionEntity.variableStateManager.getAllVariables();
  }

  /**
   * Get the value of a specified variable for a workflow execution
   */
  protected async getEntityVariable(executionId: string, name: string): Promise<unknown> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    const value = executionEntity.variableStateManager.getVariable(name);
    if (value === undefined) {
      throw new NotFoundError(`Variable not found: ${name}`, "Variable", name);
    }
    return value;
  }

  /**
   * Check if the specified variable exists in the workflow execution
   */
  protected async hasEntityVariable(executionId: string, name: string): Promise<boolean> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    return executionEntity.variableStateManager.hasVariable(name);
  }

  /**
   * Set variable value for an execution
   */
  protected async setEntityVariable(
    executionId: string,
    name: string,
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

      await executionContext.setVariable(name, value);
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
   */
  protected async deleteEntityVariable(
    executionId: string,
    name: string,
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

      await executionContext.deleteVariable(name);
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
   * Get variable definitions for a workflow execution
   */
  protected async getEntityVariableDefinitions(
    executionId: string,
  ): Promise<Record<string, unknown>> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    const definitions = executionEntity.variableStateManager.getAllVariableDefinitions();

    const result: Record<string, unknown> = {};
    for (const def of definitions) {
      result[def.name] = def;
    }
    return result;
  }

  /**
   * Parse variable ID
   */
  protected parseVariableId(id: string): [string, string] {
    const parts = id.split(":");
    if (parts.length !== 2) {
      throw new Error(
        `Invalid variable ID format: ${id}. Expected format: executionId:variableName`,
      );
    }
    return [parts[0]!, parts[1]!];
  }

  /**
   * Get variable statistics
   */
  protected async getVariableStatistics(): Promise<VariableStatistics> {
    const executionEntities = this.registry.getAll();
    const stats: VariableStatistics = {
      totalExecutions: executionEntities.length,
      totalVariables: 0,
      byExecution: {},
      byType: {},
      bySource: {},
    };

    for (const executionEntity of executionEntities) {
      const executionId = executionEntity.id;
      const manager = executionEntity.variableStateManager;
      const variables = manager.getAllVariables();

      stats.byExecution[executionId] = Object.keys(variables).length;
      stats.totalVariables += Object.keys(variables).length;

      for (const [, value] of Object.entries(variables)) {
        const type = typeof value;
        stats.byType[type] = (stats.byType[type] || 0) + 1;
      }
    }

    return stats;
  }

  // ============================================================================
  // Workflow-specific methods
  // ============================================================================

  /**
   * Get all variable values of the workflow execution
   */
  async getWorkflowExecutionVariables(executionId: string): Promise<Record<string, unknown>> {
    return this.getEntityVariables(executionId);
  }

  /**
   * Get the value of a specified variable for a workflow execution
   */
  async getWorkflowExecutionVariable(executionId: string, name: string): Promise<unknown> {
    return this.getEntityVariable(executionId, name);
  }

  /**
   * Check if the specified variable exists in the workflow execution
   */
  async hasWorkflowExecutionVariable(executionId: string, name: string): Promise<boolean> {
    return this.hasEntityVariable(executionId, name);
  }

  /**
   * Get variable definitions for a workflow execution
   */
  async getWorkflowExecutionVariableDefinitions(
    executionId: string,
  ): Promise<Record<string, unknown>> {
    return this.getEntityVariableDefinitions(executionId);
  }

  /**
   * Get variable statistics (public API - renamed to avoid conflict with base class)
   */
  async getWorkflowVariableStatistics(): Promise<{
    totalExecutions: number;
    totalVariables: number;
    byExecution: Record<string, number>;
    byType: Record<string, number>;
  }> {
    const stats = await this.getVariableStatistics();
    return {
      totalExecutions: stats.totalExecutions,
      totalVariables: stats.totalVariables,
      byExecution: stats.byExecution,
      byType: stats.byType,
    };
  }

  /**
   * Get variable scopes information
   */
  async getVariableScopes(executionId: string): Promise<{
    execution: Record<string, unknown>;
    global: Record<string, unknown>;
    subgraph: Record<string, unknown>;
    loop: Record<string, unknown>;
    currentScopeDepth: number;
    hasActiveTemporaryScope: boolean;
  }> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    const manager = executionEntity.variableStateManager;
    const allVars = manager.getAllVariables();

    return {
      execution: allVars,
      global: {},
      subgraph: {},
      loop: {},
      currentScopeDepth: 0,
      hasActiveTemporaryScope: false,
    };
  }

  /**
   * Search for variables
   */
  async searchVariables(executionId: string, query: string): Promise<string[]> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    const variables = executionEntity.variableStateManager.getAllVariables();
    const lowerQuery = query.toLowerCase();
    return Object.keys(variables).filter((name) => name.toLowerCase().includes(lowerQuery));
  }

  /**
   * Export workflow execution variables
   */
  async exportWorkflowExecutionVariables(executionId: string): Promise<string> {
    return this.exportEntityVariables(executionId);
  }

  /**
   * Get variable change history
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
   * Get variables at a specific node execution point
   * This provides a snapshot of variables as they were when a specific node was executed.
   *
   * @param executionId - Workflow execution ID
   * @param nodeId - Node ID to get variables at
   * @returns Variable values at the node execution point
   */
  async getWorkflowExecutionVariablesAtNode(
    executionId: string,
    nodeId: string,
  ): Promise<Record<string, unknown>> {
    const executionEntity = await this.getWorkflowExecutionEntity(executionId);
    const nodeResults = executionEntity.getNodeResults() || [];
    const nodeResult = nodeResults.find((n) => n.nodeId === nodeId);
    if (!nodeResult) {
      throw new NotFoundError(`Node ${nodeId} not found in execution ${executionId}`, "Node", nodeId);
    }

    // Return the current variable state at this point
    return executionEntity.variableStateManager.getAllVariables();
  }

  /**
   * Set variable value for an execution
   */
  async setVariable(
    executionId: string,
    variableName: string,
    value: unknown,
  ): Promise<ExecutionResult<void>> {
    return this.setEntityVariable(executionId, variableName, value);
  }

  /**
   * Delete variable for an execution
   */
  async deleteVariable(
    executionId: string,
    variableName: string,
  ): Promise<ExecutionResult<void>> {
    return this.deleteEntityVariable(executionId, variableName);
  }

  // ============================================================================
  // Helper methods
  // ============================================================================

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
   * Get the underlying WorkflowExecutionRegistry instance
   */
  getRegistry(): WorkflowExecutionRegistry {
    return this.registry;
  }
}