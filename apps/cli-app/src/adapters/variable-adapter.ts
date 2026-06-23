/**
 * Variable Adapter
 * Encapsulates SDK API calls related to variables
 */

import { BaseAdapter } from "./base-adapter.js";

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
      return await api.get(`${executionId}:${variableName}`);
    }, "Get variable");
  }

  /**
   * Set variable value
   */
  async setVariable(executionId: string, variableName: string, value: unknown): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      await api.setVariable(executionId, variableName, value);

      this.output.infoLog(`Variable set: ${variableName}`);
    }, "Set variable");
  }

  /**
   * List all variables of a workflow execution
   */
  async listVariables(executionId: string): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      return await api.getAll({ executionId }) as unknown as Record<string, unknown>;
    }, "List variables");
  }

  /**
   * Delete variable
   */
  async deleteVariable(executionId: string, variableName: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      await api.deleteVariable(executionId, variableName);

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
      const definitions = await api.getWorkflowExecutionVariableDefinitions(executionId);
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
