/**
 * Script Adapter
 * Manage scripts via the SDK ScriptRegistryAPI.
 */

import { BaseAdapter, type QueryOptions, type PaginatedResponse } from "./base-adapter.js";
import { findByIdOrThrow } from "@wf-agent/runtime/adapters";

export class ScriptAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Script";
  }

  async list(query?: QueryOptions): Promise<PaginatedResponse<Record<string, any>>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("list", query);
      const api = this.sdk.scripts;
      const scripts = await api.getAll();
      const items = scripts.map((s: any) => ({
        id: s.id,
        name: s.name,
        type: s.type,
        description: s.description,
        language: s.language,
      }));
      return this.applyPagination(items, query);
    }, "list scripts");
  }

  async get(id: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("get", { id });
      if (!id || id.trim().length === 0) throw new Error("Script ID is required");
      const script = await findByIdOrThrow(this.sdk.scripts, id, "Script");
      return script as any;
    }, `get script ${id}`);
  }
}