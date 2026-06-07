/**
 * Template Adapter
 * Wraps template-related SDK API calls
 */

import { BaseAdapter } from "./base-adapter.js";
import { resolve, join, extname } from "path";
import { CLINotFoundError } from "../types/cli-types.js";
import { parseNodeTemplate, parseTriggerTemplate, getData, isFailure, getError } from "@wf-agent/sdk/api";
import { loadConfigFile } from "@wf-agent/config-processor";
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
      const { readdir } = await import("fs/promises");

      const dir = options.configDir || "./configs/templates/node-templates";
      const files: string[] = [];

      const scanDir = async (currentDir: string) => {
        const entries = await readdir(currentDir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = join(currentDir, entry.name);
          if (entry.isDirectory() && options.recursive !== false) {
            await scanDir(fullPath);
          } else if (entry.isFile()) {
            const ext = extname(entry.name).toLowerCase();
            if (ext === ".toml" || ext === ".json") {
              if (!options.filePattern || options.filePattern.test(entry.name)) {
                files.push(fullPath);
              }
            }
          }
        }
      };

      await scanDir(dir);

      const success: NodeTemplate[] = [];
      const failures: Array<{ filePath: string; error: string }> = [];

      const api = this.sdk.nodeTemplates;
      for (const file of files) {
        try {
          const { content, format } = await loadConfigFile(file);
          const template = parseNodeTemplate(content, format);
          await api.create(template);
          success.push(template);
          // Output to stdout for user visibility and test verification, also log for audit
          this.logOperation(`Node template is registered: ${template.name}`);
        } catch (error) {
          failures.push({
            filePath: file,
            error: error instanceof Error ? error.message : String(error),
          });
          // Output to stderr for user visibility and test verification, also log for audit
          this.logOperationFailure(`Failed to register node template: ${file}`);
        }
      }

      return { success, failures };
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
      const { readdir } = await import("fs/promises");

      const dir = options.configDir || "./configs/templates/trigger-templates";
      const files: string[] = [];

      const scanDir = async (currentDir: string) => {
        const entries = await readdir(currentDir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = join(currentDir, entry.name);
          if (entry.isDirectory() && options.recursive !== false) {
            await scanDir(fullPath);
          } else if (entry.isFile()) {
            const ext = extname(entry.name).toLowerCase();
            if (ext === ".toml" || ext === ".json") {
              if (!options.filePattern || options.filePattern.test(entry.name)) {
                files.push(fullPath);
              }
            }
          }
        }
      };

      await scanDir(dir);

      const success: TriggerTemplate[] = [];
      const failures: Array<{ filePath: string; error: string }> = [];

      const api = this.sdk.triggerTemplates;
      for (const file of files) {
        try {
          const { content, format } = await loadConfigFile(file);
          const template = parseTriggerTemplate(content, format);
          await api.create(template);
          success.push(template);
          // Output to stdout for user visibility and test verification, also log for audit
          this.logOperation(`Trigger template is registered: ${template.name}`);
        } catch (error) {
          failures.push({
            filePath: file,
            error: error instanceof Error ? error.message : String(error),
          });
          // Output to stderr for user visibility and test verification, also log for audit
          this.logOperationFailure(`Failed to register trigger template: ${file}`);
        }
      }

      return { success, failures };
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
      
      const result = await api.getAll(nodeFilter);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const templates = getData(result) as NodeTemplate[];

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
      
      const result = await api.getAll(triggerFilter);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const templates = getData(result) as TriggerTemplate[];

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
      const result = await api.get(id);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const template = getData(result);

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
      const result = await api.get(id);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const template = getData(result);

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
