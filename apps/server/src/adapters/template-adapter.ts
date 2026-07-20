/**
 * Template Adapter
 * Manage node templates and trigger templates via the SDK.
 */

import { BaseAdapter, type QueryOptions, type PaginatedResponse } from "./base-adapter.js";
import { findByIdOrThrow, batchRegisterFromDir } from "@wf-agent/runtime/adapters";
import { loadConfigFile } from "@wf-agent/runtime/config";
import { parseNodeTemplate, parseTriggerTemplate } from "@wf-agent/sdk/api";
import { resolve } from "path";

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

  async instantiateNodeTemplate(id: string, params?: Record<string, any>): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("instantiateNodeTemplate", { id, params });
      if (!id || id.trim().length === 0) throw new Error("Template ID is required");
      const template = await findByIdOrThrow(this.sdk.nodeTemplates, id, "NodeTemplate");
      const workflowsApi = this.sdk.workflows as any;
      const workflow = await workflowsApi.createFromTemplate(id, params);
      return { template, workflow } as any;
    }, `instantiate node template ${id}`);
  }

  async registerNodeTemplateFromFile(filePath: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("registerNodeTemplateFromFile", { filePath });
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const template = parseNodeTemplate(content, format);
      await this.sdk.nodeTemplates.create(template);
      return template as any;
    }, "register node template from file");
  }

  async registerNodeTemplatesFromDirectory(options: {
    configDir: string;
    recursive?: boolean;
    filePattern?: RegExp;
  }): Promise<{
    success: any[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("registerNodeTemplatesFromDirectory", { configDir: options.configDir });
      return await batchRegisterFromDir({
        configDir: options.configDir,
        recursive: options.recursive,
        filePattern: options.filePattern,
        loadAndParse: async (file) => {
          const { content, format } = await loadConfigFile(file);
          return parseNodeTemplate(content, format);
        },
        register: async (template) => {
          await this.sdk.nodeTemplates.create(template);
        },
        onSuccess: (template) => {
          this.logOperation(`Node template registered: ${template.name}`);
        },
        onFailure: (file) => {
          this.logOperation(`Failed to register node template: ${file}`);
        },
      });
    }, "batch register node templates");
  }

  async registerTriggerTemplateFromFile(filePath: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("registerTriggerTemplateFromFile", { filePath });
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const template = parseTriggerTemplate(content, format);
      await this.sdk.triggerTemplates.create(template);
      return template as any;
    }, "register trigger template from file");
  }

  async registerTriggerTemplatesFromDirectory(options: {
    configDir: string;
    recursive?: boolean;
    filePattern?: RegExp;
  }): Promise<{
    success: any[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("registerTriggerTemplatesFromDirectory", { configDir: options.configDir });
      return await batchRegisterFromDir({
        configDir: options.configDir,
        recursive: options.recursive,
        filePattern: options.filePattern,
        loadAndParse: async (file) => {
          const { content, format } = await loadConfigFile(file);
          return parseTriggerTemplate(content, format);
        },
        register: async (template) => {
          await this.sdk.triggerTemplates.create(template);
        },
        onSuccess: (template) => {
          this.logOperation(`Trigger template registered: ${template.name}`);
        },
        onFailure: (file) => {
          this.logOperation(`Failed to register trigger template: ${file}`);
        },
      });
    }, "batch register trigger templates");
  }

  async deleteNodeTemplate(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("deleteNodeTemplate", { id });
      if (!id || id.trim().length === 0) throw new Error("Template ID is required");
      await findByIdOrThrow(this.sdk.nodeTemplates, id, "NodeTemplate");
      await this.sdk.nodeTemplates.delete(id);
    }, `delete node template ${id}`);
  }

  async deleteTriggerTemplate(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("deleteTriggerTemplate", { id });
      if (!id || id.trim().length === 0) throw new Error("Template ID is required");
      await findByIdOrThrow(this.sdk.triggerTemplates, id, "TriggerTemplate");
      await this.sdk.triggerTemplates.delete(id);
    }, `delete trigger template ${id}`);
  }
}