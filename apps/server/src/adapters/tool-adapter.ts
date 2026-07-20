/**
 * Tool Adapter
 * Manage tools via the SDK ToolRegistryAPI.
 */

import { BaseAdapter, type QueryOptions, type PaginatedResponse } from "./base-adapter.js";
import { findByIdOrThrow, batchRegisterFromDir } from "@wf-agent/runtime/adapters";
import { loadConfigFile } from "@wf-agent/runtime/config";
import { resolve } from "path";

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

  async registerFromFile(filePath: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("registerFromFile", { filePath });
      const fullPath = resolve(process.cwd(), filePath);
      const { content } = await loadConfigFile(fullPath);
      const tool = JSON.parse(content);
      await this.sdk.tools.create(tool);
      return tool as any;
    }, "register tool from file");
  }

  async registerFromDirectory(options: {
    configDir: string;
    recursive?: boolean;
    filePattern?: RegExp;
  }): Promise<{
    success: any[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("registerFromDirectory", { configDir: options.configDir });
      return await batchRegisterFromDir({
        configDir: options.configDir,
        recursive: options.recursive,
        filePattern: options.filePattern,
        loadAndParse: async (file) => {
          const { content } = await loadConfigFile(file);
          return JSON.parse(content);
        },
        register: async (tool) => {
          await this.sdk.tools.create(tool);
        },
        onSuccess: (tool) => {
          this.logOperation(`Tool registered: ${tool.name}`);
        },
        onFailure: (file) => {
          this.logOperation(`Failed to register tool: ${file}`);
        },
      });
    }, "batch register tools");
  }

  async delete(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("delete", { id });
      if (!id || id.trim().length === 0) throw new Error("Tool ID is required");
      await findByIdOrThrow(this.sdk.tools, id, "Tool");
      await this.sdk.tools.delete(id);
    }, `delete tool ${id}`);
  }
}