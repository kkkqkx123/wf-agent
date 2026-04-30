/**
 * Variable Adapter
 * Encapsulates SDK API calls related to variables
 */

import { BaseAdapter } from "./base-adapter.js";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Variable Adapter
 */
export class VariableAdapter extends BaseAdapter {
  /**
   * Get variable value
   */
  async getVariable(executionId: string, variableName: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const result = await api.get(`${executionId}:${variableName}`);
      const variable = (result as any).data || result;
      return variable;
    }, "Get variable");
  }

  /**
   * Set variable value
   */
  async setVariable(executionId: string, variableName: string, value: any): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const registry = api.getRegistry();
      const executionContext = registry.get(executionId);
      if (!executionContext) {
        throw new CLINotFoundError(`WorkflowExecution not found: ${executionId}`, "WorkflowExecution", executionId);
      }
      await executionContext.setVariable(variableName, value);
      this.output.infoLog(`Variable set: ${variableName}`);
    }, "Set variable");
  }

  /**
   * List all variables of a workflow execution
   */
  async listVariables(executionId: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const result = await api.getAll({ executionId });
      const variables = (result as any).data || result;
      return variables as Record<string, any>;
    }, "List variables");
  }

  /**
   * Delete variable
   */
  async deleteVariable(executionId: string, variableName: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const registry = api.getRegistry();
      const executionContext = registry.get(executionId);
      if (!executionContext) {
        throw new CLINotFoundError(`WorkflowExecution not found: ${executionId}`, "WorkflowExecution", executionId);
      }
      await executionContext.deleteVariable(variableName);
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
    defaultValue?: any;
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
        defaultValue?: any;
        required?: boolean;
      } | null;
    }, "Get variable definition");
  }
}
