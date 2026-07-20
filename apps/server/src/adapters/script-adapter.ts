/**
 * Script Adapter
 * Manage scripts via the SDK ScriptRegistryAPI.
 */

import { BaseAdapter, type QueryOptions, type PaginatedResponse } from "./base-adapter.js";
import { findByIdOrThrow, batchRegisterFromDir } from "@wf-agent/runtime/adapters";
import { loadConfigFile } from "@wf-agent/runtime/config";
import { parseScript } from "@wf-agent/sdk/api";
import { resolve } from "path";

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

  async registerFromFile(filePath: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("registerFromFile", { filePath });
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const script = parseScript(content, format);
      await this.sdk.scripts.create(script);
      return script as any;
    }, "register script from file");
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
          const { content, format } = await loadConfigFile(file);
          return parseScript(content, format);
        },
        register: async (script) => {
          await this.sdk.scripts.create(script);
        },
        onSuccess: (script) => {
          this.logOperation(`Script registered: ${script.name}`);
        },
        onFailure: (file) => {
          this.logOperation(`Failed to register script: ${file}`);
        },
      });
    }, "batch register scripts");
  }

  async delete(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("delete", { id });
      if (!id || id.trim().length === 0) throw new Error("Script ID is required");
      await findByIdOrThrow(this.sdk.scripts, id, "Script");
      await this.sdk.scripts.delete(id);
    }, `delete script ${id}`);
  }
}