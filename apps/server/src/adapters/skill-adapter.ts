/**
 * Skill Adapter
 * Manage skills via the SDK SkillRegistryAPI.
 */

import { BaseAdapter, type QueryOptions, type PaginatedResponse } from "./base-adapter.js";
import { isSuccess, getError } from "@wf-agent/sdk/api";

export class SkillAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Skill";
  }

  async list(query?: QueryOptions): Promise<PaginatedResponse<Record<string, any>>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("list", query);
      const api = this.sdk.skills;
      const skills = await api.getAll();
      const items = skills.map((s: any) => ({
        name: s.name,
        version: s.version,
        description: s.description,
        tags: s.tags,
        author: s.author,
      }));
      return this.applyPagination(items, query);
    }, "list skills");
  }

  async get(name: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("get", { name });
      if (!name || name.trim().length === 0) throw new Error("Skill name is required");
      const api = this.sdk.skills;
      const skill = await api.get(name);
      if (!skill) throw new Error(`Skill not found: ${name}`);
      return skill as any;
    }, `get skill ${name}`);
  }

  async loadContent(name: string, variables?: Record<string, unknown>): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("loadContent", { name });
      const api = this.sdk.skills;
      const result = await api.loadContent(name, {
        context: variables ? { variables } : undefined,
      });
      if (!isSuccess(result)) throw getError(result) || new Error("Failed to load skill content");
      // ExecutionResult<string> - get the data from the result
      const content = result.result?.isOk() ? result.result.value : "";
      return content || "";
    }, `load skill content ${name}`);
  }

  async register(name: string, metadata: Record<string, any>): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("register", { name });
      const api = this.sdk.skills as any;
      const result = await api.register(name, metadata);
      if (!isSuccess(result)) throw getError(result) || new Error("Failed to register skill");
      return { name, ...metadata } as any;
    }, `register skill ${name}`);
  }

  async unregister(name: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("unregister", { name });
      if (!name || name.trim().length === 0) throw new Error("Skill name is required");
      const api = this.sdk.skills as any;
      const result = await api.unregister(name);
      if (!isSuccess(result)) throw getError(result) || new Error("Failed to unregister skill");
    }, `unregister skill ${name}`);
  }

  async enable(name: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("enable", { name });
      if (!name || name.trim().length === 0) throw new Error("Skill name is required");
      const api = this.sdk.skills as any;
      await api.enable(name);
    }, `enable skill ${name}`);
  }

  async disable(name: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("disable", { name });
      if (!name || name.trim().length === 0) throw new Error("Skill name is required");
      const api = this.sdk.skills as any;
      await api.disable(name);
    }, `disable skill ${name}`);
  }

  async listResources(): Promise<Record<string, any>[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listResources");
      const api = this.sdk.skills as any;
      const resources = await api.getResources();
      return (resources || []) as any[];
    }, "list skill resources");
  }
}