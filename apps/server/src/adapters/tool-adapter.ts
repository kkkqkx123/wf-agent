/**
 * Tool Adapter
 * Manage tools via the SDK ToolRegistryAPI.
 */

import { BaseAdapter, type QueryOptions, type PaginatedResponse } from "./base-adapter.js";
import { findByIdOrThrow } from "@wf-agent/runtime/adapters";

export class ToolAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Tool";
  }

  async list(query?: QueryOptions): Promise<PaginatedResponse<Record<string, any>>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("list", query);
      const api = this.sdk.tools;
      const tools = await api.getAll();
      const items = tools.map((t: any) => ({
        id: t.id,
        name: t.name || t.id,
        type: t.type,
        description: t.description,
        source: t.source,
      }));
      return this.applyPagination(items, query);
    }, "list tools");
  }

  async get(id: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("get", { id });
      if (!id || id.trim().length === 0) {
        throw new Error("Tool ID is required");
      }
      const tool = await findByIdOrThrow(this.sdk.tools, id, "Tool");
      return tool as any;
    }, `get tool ${id}`);
  }

  async validateTool(id: string, config?: Record<string, any>): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("validateTool", { id });
      if (!id || id.trim().length === 0) {
        throw new Error("Tool ID is required");
      }
      const tool = await findByIdOrThrow(this.sdk.tools, id, "Tool");
      return { id, valid: true, tool: tool as any, config };
    }, `validate tool ${id}`);
  }
}