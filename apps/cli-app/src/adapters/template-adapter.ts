/**
 * Template Adapter
 * Wraps template-related SDK API calls
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve } from "path";
import { CLINotFoundError } from "../types/cli-types.js";
import { parseNodeTemplate, parseTriggerTemplate } from "@wf-agent/sdk/api";
import { loadConfigFile } from "@wf-agent/config-processor";
import { batchRegisterFromDir } from "@wf-agent/runtime/adapters";
import type { NodeTemplate, TriggerTemplate, NodeTemplateSummary, TriggerTemplateSummary } from "@wf-agent/types";

/**
 * Template Adapter
 */
export class TemplateAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Register node template from file
   * @param filePath Configuration file path
   * @returns Node template definition
   */
  async registerNodeTemplateFromFile(filePath: string): Promise<NodeTemplate> {
    return this.executeWithErrorHandling(async () => {
      // Use SDK to load the configuration.
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const template = parseNodeTemplate(content, format);

      const api = this.sdk.nodeTemplates;
      await api.create(template);

      // Output to stdout for user visibility and test verification, also log for audit
      this.logOperation(`Node template is registered: ${template.name}`);
      return template;
    }, "Register Node Template");
  }

  /**
   * Batch register node templates from directory
   * Uses runtime's batchRegisterFromDir to eliminate duplicated scan/load/register logic.
   * @param options Load options
   * @returns Registration result
   */
  async registerNodeTemplatesFromDirectory(options: {
    configDir: string;
    recursive?: boolean;
    filePattern?: RegExp;
  }): Promise<{
    success: NodeTemplate[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      return await batchRegisterFromDir({
        configDir: options.configDir || "./configs/templates/node-templates",
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
          this.logOperation(`Node template is registered: ${template.name}`);
        },
        onFailure: (file) => {
          this.logOperationFailure(`Failed to register node template: ${file}`);
        },
      });
    }, "Batch registration of node templates");
  }

  /**
   * Register trigger template from file
   * @param filePath Configuration file path
   * @returns Trigger template definition
   */
  async registerTriggerTemplateFromFile(filePath: string): Promise<TriggerTemplate> {
    return this.executeWithErrorHandling(async () => {
      // Use SDK to load the configuration.
      const fullPath = resolve(process.cwd(), filePath);
      const { content, format } = await loadConfigFile(fullPath);
      const template = parseTriggerTemplate(content, format);

      const api = this.sdk.triggerTemplates;
      await api.create(template);

      // Output to stdout for user visibility and test verification, also log for audit
      this.logOperation(`Trigger template is registered: ${template.name}`);
      return template;
    }, "Register Trigger Template");
  }

  /**
   * Batch register trigger templates from directory
   * Uses runtime's batchRegisterFromDir to eliminate duplicated scan/load/register logic.
   * @param options Load options
   * @returns Registration result
   */
  async registerTriggerTemplatesFromDirectory(options: {
    configDir: string;
    recursive?: boolean;
    filePattern?: RegExp;
  }): Promise<{
    success: TriggerTemplate[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      return await batchRegisterFromDir({
        configDir: options.configDir || "./configs/templates/trigger-templates",
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
          this.logOperation(`Trigger template is registered: ${template.name}`);
        },
        onFailure: (file) => {
          this.logOperationFailure(`Failed to register trigger template: ${file}`);
        },
      });
    }, "Batch registration of trigger templates");
  }

  /**
   * List all node templates
   */
  async listNodeTemplates(filter?: Record<string, unknown>): Promise<NodeTemplateSummary[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.nodeTemplates;

      // Convert filter to NodeTemplateFilter type
      const nodeFilter = filter ? {
        name: filter['name'] as string | undefined,
        nodeType: filter['nodeType'] as string | undefined,
        category: filter['category'] as string | undefined,
        tags: filter['tags'] as string[] | undefined,
      } : undefined;

      const templates = await api.getAll(nodeFilter);

      // Transform templates into summary format.
      const summaries: NodeTemplateSummary[] = templates.map((tmpl: NodeTemplate) => ({
        name: tmpl.name,
        type: tmpl.type,
        description: tmpl.description,
        category: (tmpl.metadata?.['category'] as string) || undefined,
        tags: (tmpl.metadata?.['tags'] as string[]) || undefined,
        createdAt: tmpl.createdAt,
        updatedAt: tmpl.updatedAt,
      }));

      return summaries;
    }, "List node templates");
  }

  /**
   * List all trigger templates
   */
  async listTriggerTemplates(filter?: Record<string, unknown>): Promise<TriggerTemplateSummary[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggerTemplates;

      // Convert filter to TriggerTemplateFilter type
      const triggerFilter = filter ? {
        name: filter['name'] as string | undefined,
        keyword: filter['keyword'] as string | undefined,
        triggerType: filter['triggerType'] as string | undefined,
        category: filter['category'] as string | undefined,
        tags: filter['tags'] as string[] | undefined,
      } : undefined;

      const templates = await api.getAll(triggerFilter);

      // Transform the text into summary format.
      const summaries: TriggerTemplateSummary[] = templates.map((tmpl: TriggerTemplate) => ({
        name: tmpl.name,
        description: tmpl.description,
        category: (tmpl.metadata?.['category'] as string) || undefined,
        tags: (tmpl.metadata?.['tags'] as string[]) || undefined,
        createdAt: tmpl.createdAt,
        updatedAt: tmpl.updatedAt,
      }));

      return summaries;
    }, "List trigger templates");
  }

  /**
   * Get node template details
   */
  async getNodeTemplate(id: string): Promise<NodeTemplate> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.nodeTemplates;
      const template = await api.get(id);

      if (!template) {
        throw new CLINotFoundError(`Node template not found: ${id}`, "NodeTemplate", id);
      }

      return template;
    }, "Get node template details");
  }

  /**
   * Get trigger template details
   */
  async getTriggerTemplate(id: string): Promise<TriggerTemplate> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggerTemplates;
      const template = await api.get(id);

      if (!template) {
        throw new CLINotFoundError(`Trigger template not found: ${id}`, "TriggerTemplate", id);
      }

      return template;
    }, "Get trigger template details");
  }

  /**
   * Delete node template
   */
  async deleteNodeTemplate(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.nodeTemplates;
      await api.delete(id);

      // Output to stdout for user visibility and test verification, also log for audit
      this.logOperation(`Node template is deleted: ${id}`);
    }, "Delete the node template.");
  }

  /**
   * Delete trigger template
   */
  async deleteTriggerTemplate(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggerTemplates;
      await api.delete(id);

      // Output to stdout for user visibility and test verification, also log for audit
      this.logOperation(`Trigger template is deleted: ${id}`);
    }, "Delete the trigger template");
  }
}
