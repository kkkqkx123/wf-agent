/**
 * Template Adapter
 * Manage node templates and trigger templates via the SDK.
 */

import { BaseAdapter, type QueryOptions, type PaginatedResponse } from "./base-adapter.js";
import { findByIdOrThrow } from "@wf-agent/runtime/adapters";

export class TemplateAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Template";
  }

  async listNodeTemplates(query?: QueryOptions): Promise<PaginatedResponse<Record<string, any>>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listNodeTemplates", query);
      const api = this.sdk.nodeTemplates;
      const templates = await api.getAll();
      const items = templates.map((t: any) => ({
        id: t.id,
        name: t.name,
        type: t.nodeType,
        description: t.description,
        category: t.category,
      }));
      return this.applyPagination(items, query);
    }, "list node templates");
  }

  async getNodeTemplate(id: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getNodeTemplate", { id });
      if (!id || id.trim().length === 0) throw new Error("Template ID is required");
      const template = await findByIdOrThrow(this.sdk.nodeTemplates, id, "NodeTemplate");
      return template as any;
    }, `get node template ${id}`);
  }

  async listTriggerTemplates(query?: QueryOptions): Promise<PaginatedResponse<Record<string, any>>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listTriggerTemplates", query);
      const api = this.sdk.triggerTemplates;
      const templates = await api.getAll();
      const items = templates.map((t: any) => ({
        id: t.id,
        name: t.name,
        type: t.triggerType,
        description: t.description,
      }));
      return this.applyPagination(items, query);
    }, "list trigger templates");
  }

  async getTriggerTemplate(id: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getTriggerTemplate", { id });
      if (!id || id.trim().length === 0) throw new Error("Template ID is required");
      const template = await findByIdOrThrow(this.sdk.triggerTemplates, id, "TriggerTemplate");
      return template as any;
    }, `get trigger template ${id}`);
  }
}