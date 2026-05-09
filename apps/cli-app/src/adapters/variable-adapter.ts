/**
 * Variable Adapter
 * Encapsulates SDK API calls related to variables
 */

import { BaseAdapter } from "./base-adapter.js";
import { CLINotFoundError } from "../types/cli-types.js";
import { getData, isFailure, getError } from "@wf-agent/sdk";

/**
 * Variable Adapter
 */
export class VariableAdapter extends BaseAdapter {
  /**
   * Get variable value
   */
  async getVariable(executionId: string, variableName: string): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const result = await api.get(`${executionId}:${variableName}`);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result);
    }, "Get variable");
  }

  /**
   * Set variable value
   */
  async setVariable(executionId: string, variableName: string, value: unknown): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const result = await api.setVariable(executionId, variableName, value);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      this.output.infoLog(`Variable set: ${variableName}`);
    }, "Set variable");
  }

  /**
   * List all variables of a workflow execution
   */
  async listVariables(executionId: string): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const result = await api.getAll({ executionId });
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result) as Record<string, unknown>;
    }, "List variables");
  }

  /**
   * Delete variable
   */
  async deleteVariable(executionId: string, variableName: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const result = await api.deleteVariable(executionId, variableName);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      this.output.infoLog(`Variable deleted: ${variableName}`);
    }, "Delete variable");
  }

  /**
   * Get variable definition information
   */
  async getVariableDefinition(
    executionId: string,
    variableName: string,
  ): Promise<{
    name: string;
    type: string;
    description?: string;
    defaultValue?: unknown;
    required?: boolean;
  } | null> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const definitions = await api.getWorkflowExecutionVariableDefinitions();
      const definition = definitions[variableName] || null;
      return definition as {
        name: string;
        type: string;
        description?: string;
        defaultValue?: unknown;
        required?: boolean;
      } | null;
    }, "Get variable definition");
  }
}
