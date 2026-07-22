/**
 * BaseVariableResourceAPI - Shared base class for variable resource management
 *
 * Provides a unified base implementation for variable CRUD and query operations.
 * Both Agent and Workflow versions extend this class, providing only entity-specific
 * type annotations and method naming.
 */

import { QueryableResourceAPI } from "./generic-resource-api.js";
import type { ExecutionResult } from "../types/execution-result.js";
import { success, failure } from "../types/execution-result.js";
import { SDKError } from "@wf-agent/types";

/**
 * Base variable filter
 */
export interface BaseVariableFilter {
  /** Filter by source */
  source?: string;
  /** Filter by type */
  type?: string;
}

/**
 * Variable statistics
 */
export interface VariableStatistics {
  /** Total number of executions */
  totalExecutions: number;
  /** Total number of variables */
  totalVariables: number;
  /** Distribution by execution */
  byExecution: Record<string, number>;
  /** Distribution by type */
  byType: Record<string, number>;
  /** Distribution by source */
  bySource: Record<string, number>;
}

/**
 * Base context variable with scope information
 */
export interface BaseContextVariable {
  name: string;
  value: unknown;
  scope: "execution" | "iteration" | "global";
}

/**
 * BaseVariableResourceAPI - Shared base class for variable resource management
 *
 * Entity-specific subclasses must implement:
 * - getEntityVariables(entityId): Get all variables for an entity
 * - getEntityVariable(entityId, name): Get a specific variable
 * - hasEntityVariable(entityId, name): Check if variable exists
 * - setEntityVariable(entityId, name, value): Set a variable
 * - deleteEntityVariable(entityId, name): Delete a variable
 * - getEntityVariableDefinitions(entityId): Get variable definitions
 * - getEntityIdFromVariableId(id): Parse variable ID into entity ID and name
 * - getVariableStatistics(): Get global statistics
 *
 * Shared methods provided by this base class:
 * - exportVariables / importVariables / batchSetVariables
 * - getVariableScopes / getInputVariables / getOutputVariables
 */
export abstract class BaseVariableResourceAPI<TFilter extends BaseVariableFilter> extends QueryableResourceAPI<
  unknown,
  string,
  TFilter
> {
  /**
   * Get all variables for an entity
   * @param entityId Entity ID
   * @returns Record of variables
   */
  protected abstract getEntityVariables(entityId: string): Record<string, unknown> | Promise<Record<string, unknown>>;

  /**
   * Get a specific variable value
   * @param entityId Entity ID
   * @param name Variable name
   * @returns Variable value or undefined
   */
  protected abstract getEntityVariable(entityId: string, name: string): Promise<unknown>;

  /**
   * Check if a variable exists
   * @param entityId Entity ID
   * @param name Variable name
   * @returns Whether the variable exists
   */
  protected abstract hasEntityVariable(entityId: string, name: string): Promise<boolean>;

  /**
   * Set a variable value
   * @param entityId Entity ID
   * @param name Variable name
   * @param value Variable value
   * @returns Execution result
   */
  protected abstract setEntityVariable(
    entityId: string,
    name: string,
    value: unknown,
  ): Promise<ExecutionResult<void>>;

  /**
   * Delete a variable
   * @param entityId Entity ID
   * @param name Variable name
   * @returns Execution result
   */
  protected abstract deleteEntityVariable(
    entityId: string,
    name: string,
  ): Promise<ExecutionResult<void>>;

  /**
   * Get variable definitions for an entity
   * @param entityId Entity ID
   * @returns Record of variable definitions
   */
  protected abstract getEntityVariableDefinitions(
    entityId: string,
  ): Promise<Record<string, unknown>>;

  /**
   * Parse a variable ID into entity ID and variable name
   * @param id Variable ID (format: entityId:variableName)
   * @returns [entityId, variableName]
   */
  protected abstract parseVariableId(id: string): [string, string];

  /**
   * Get variable statistics
   * @returns Statistical information
   */
  protected abstract getVariableStatistics(): Promise<VariableStatistics>;

  /**
   * Get variable value by composite ID
   */
  protected async getResource(id: string): Promise<unknown | null> {
    const [entityId, variableName] = this.parseVariableId(id);
    const variables = await this.getEntityVariables(entityId);
    return variables[variableName] ?? null;
  }

  /**
   * Get all variable values (returns empty array - use specific methods instead)
   */
  protected async getAllResources(): Promise<unknown[]> {
    return [];
  }

  /**
   * Get all variables in an execution context
   * @param entityId Entity ID
   * @returns Record of variables
   */
  protected async getEntityVariablesWithContext(entityId: string): Promise<Record<string, unknown>> {
    return this.getEntityVariables(entityId);
  }

  /**
   * Get a specific variable value
   * @param entityId Entity ID
   * @param name Variable name
   * @returns Variable value
   */
  protected async getEntityVariableValue(entityId: string, name: string): Promise<unknown> {
    return this.getEntityVariable(entityId, name);
  }

  /**
   * Check if a variable exists
   * @param entityId Entity ID
   * @param name Variable name
   * @returns Whether the variable exists
   */
  protected async hasEntityVariableValue(entityId: string, name: string): Promise<boolean> {
    return this.hasEntityVariable(entityId, name);
  }

  /**
   * Search variables by name
   * @param entityId Entity ID
   * @param query Search keyword
   * @returns Array of matching variable definitions
   */
  protected async searchEntityVariables(
    entityId: string,
    query: string,
  ): Promise<unknown[]> {
    const definitions = await this.getEntityVariableDefinitions(entityId);
    const lowerQuery = query.toLowerCase();
    return Object.values(definitions).filter(
      (def: unknown) => {
        const name = (def as Record<string, unknown>)["name"];
        return typeof name === "string" && name.toLowerCase().includes(lowerQuery);
      },
    );
  }

  /**
   * Export variables for an entity as JSON
   * @param entityId Entity ID
   * @returns JSON string
   */
  protected async exportEntityVariables(entityId: string): Promise<string> {
    const variables = await this.getEntityVariables(entityId);
    return JSON.stringify(variables, null, 2);
  }

  // ============================================================================
  // Shared Batch Operations
  // ============================================================================

  /**
   * Export all variables for an execution as a record
   * @param executionId Execution ID
   * @returns Record of variable names to values
   */
  async exportVariables(executionId: string): Promise<Record<string, unknown>> {
    return this.getEntityVariables(executionId);
  }

  /**
   * Import variables into an execution from a record
   * @param executionId Execution ID
   * @param variables Record of variable names to values
   * @param options Import options
   */
  async importVariables(
    executionId: string,
    variables: Record<string, unknown>,
    options?: { overwrite?: boolean },
  ): Promise<void> {
    for (const [name, value] of Object.entries(variables)) {
      if (options?.overwrite === false) {
        const exists = await this.hasEntityVariable(executionId, name);
        if (exists) continue;
      }
      await this.setEntityVariable(executionId, name, value);
    }
  }

  /**
   * Batch set multiple variables for an execution
   * @param executionId Execution ID
   * @param variables Record of variable names to values
   * @returns Execution result
   */
  async batchSetVariables(
    executionId: string,
    variables: Record<string, unknown>,
  ): Promise<ExecutionResult<void>> {
    try {
      for (const [name, value] of Object.entries(variables)) {
        const result = await this.setEntityVariable(executionId, name, value);
        if (result.result.isErr()) {
          return failure(
            new SDKError(`Failed to set variable: ${name}`, "error", undefined, result.result.error),
            0,
          );
        }
      }
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
  // Shared Input/Output Variable Methods
  // ============================================================================
}