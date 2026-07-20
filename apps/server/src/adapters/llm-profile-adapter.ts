/**
 * LLM Profile Adapter
 * Manage LLM profiles via the SDK ProfileRegistryAPI.
 */

import { BaseAdapter, type QueryOptions, type PaginatedResponse } from "./base-adapter.js";
import { findByIdOrThrow } from "@wf-agent/runtime/adapters";
import { isSuccess, getError } from "@wf-agent/sdk/api";

export class LLMProfileAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "LLMProfile";
  }

  async list(query?: QueryOptions): Promise<PaginatedResponse<Record<string, any>>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("list", query);
      const api = this.sdk.profiles;
      const profiles = await api.getAll();
      const items = profiles.map((p: any) => ({
        id: p.id,
        name: p.name,
        provider: p.provider,
        model: p.model,
        isDefault: p.isDefault,
      }));
      return this.applyPagination(items, query);
    }, "list LLM profiles");
  }

  async get(id: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("get", { id });
      if (!id || id.trim().length === 0) throw new Error("LLM Profile ID is required");
      const profile = await findByIdOrThrow(this.sdk.profiles, id, "LLMProfile");
      return profile as any;
    }, `get LLM profile ${id}`);
  }

  async create(data: Record<string, any>): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("create", { name: data["name"] });
      if (!data["id"] || !data["name"]) throw new Error("LLM Profile id and name are required");
      const result = await this.sdk.profiles.create(data as any);
      if (!isSuccess(result)) throw getError(result) || new Error("Failed to create LLM profile");
      return data;
    }, "create LLM profile");
  }

  async update(id: string, data: Partial<Record<string, any>>): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("update", { id });
      if (!id || id.trim().length === 0) throw new Error("LLM Profile ID is required");
      await findByIdOrThrow(this.sdk.profiles, id, "LLMProfile");
      const result = await this.sdk.profiles.update(id, data as any);
      if (!isSuccess(result)) throw getError(result) || new Error("Failed to update LLM profile");
      return { id, ...data };
    }, `update LLM profile ${id}`);
  }

  async delete(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("delete", { id });
      if (!id || id.trim().length === 0) throw new Error("LLM Profile ID is required");
      await findByIdOrThrow(this.sdk.profiles, id, "LLMProfile");
      const result = await this.sdk.profiles.delete(id);
      if (!isSuccess(result)) throw getError(result) || new Error("Failed to delete LLM profile");
    }, `delete LLM profile ${id}`);
  }

  async setDefault(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("setDefault", { id });
      if (!id || id.trim().length === 0) throw new Error("LLM Profile ID is required");
      await findByIdOrThrow(this.sdk.profiles, id, "LLMProfile");
      const api = this.sdk.profiles;
      await api.setDefaultProfile(id);
    }, "set default LLM profile");
  }
}