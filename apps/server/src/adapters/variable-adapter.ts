/**
 * Variable Adapter
 * Manage workflow execution variables via the SDK.
 */

import { BaseAdapter } from "./base-adapter.js";

export class VariableAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Variable";
  }

  async listVariables(executionId: string): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listVariables", { executionId });
      const api = this.sdk.variables;
      return await api.getWorkflowExecutionVariables(executionId) as unknown as Record<string, unknown>;
    }, "List variables");
  }

  async getVariable(executionId: string, variableName: string): Promise<unknown> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getVariable", { executionId, variableName });
      const api = this.sdk.variables;
      return await api.get(`${executionId}:${variableName}`);
    }, "Get variable");
  }

  async setVariable(executionId: string, variableName: string, value: unknown): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("setVariable", { executionId, variableName });
      const api = this.sdk.variables;
      await api.setVariable(executionId, variableName, value);
    }, "Set variable");
  }

  async deleteVariable(executionId: string, variableName: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("deleteVariable", { executionId, variableName });
      const api = this.sdk.variables;
      await api.deleteVariable(executionId, variableName);
    }, "Delete variable");
  }

  async getVariableDefinition(executionId: string, variableName: string): Promise<Record<string, any> | null> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getVariableDefinition", { executionId, variableName });
      const api = this.sdk.variables;
      const value = await api.get(`${executionId}:${variableName}`);
      if (value === null || value === undefined) return null;
      return { name: variableName, type: typeof value, value };
    }, "Get variable definition");
  }
}