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
  async getVariable(threadId: string, variableName: string): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const result = await api.get(`${threadId}:${variableName}`);
      const variable = (result as any).data || result;
      return variable;
    }, "Get variable");
  }

  /**
   * Set variable value
   */
  async setVariable(threadId: string, variableName: string, value: any): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const registry = api.getRegistry();
      const threadContext = registry.get(threadId);
      if (!threadContext) {
        throw new CLINotFoundError(`Thread not found: ${threadId}`, "Thread", threadId);
      }
      await threadContext.setVariable(variableName, value);
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
  async deleteVariable(threadId: string, variableName: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.variables;
      const registry = api.getRegistry();
      const threadContext = registry.get(threadId);
      if (!threadContext) {
        throw new CLINotFoundError(`Thread not found: ${threadId}`, "Thread", threadId);
      }
      await threadContext.deleteVariable(variableName);
      this.output.infoLog(`Variable deleted: ${variableName}`);
    }, "Delete variable");
  }

  /**
   * Get variable definition information
   */
  async getVariableDefinition(
    threadId: string,
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
      const definitions = await api.getThreadVariableDefinitions();
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
