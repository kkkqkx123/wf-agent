/**
 * Hook Adapter
 * Encapsulates SDK API calls related to hook template management
 */

import { BaseAdapter } from "./base-adapter.js";
import { CLINotFoundError } from "../types/cli-types.js";
import { loadHookTemplateConfig } from "@wf-agent/config-processor";
import { resolve } from "path";
import type { HookTemplate } from "@wf-agent/types";

/**
 * Hook template filter
 */
export interface HookTemplateFilter {
  name?: string;
  hookType?: string;
  category?: string;
  tags?: string[];
}

/**
 * Hook template summary
 */
export interface HookTemplateSummary {
  name: string;
  hookType: string;
  description?: string;
  category?: string;
  tags?: string[];
  createdAt: number;
  updatedAt: number;
}

/**
 * Hook Adapter
 */
export class HookAdapter extends BaseAdapter {
  /**
   * Get the hook template registry API
   * Note: Hook templates are not directly exposed as a getter on SDKInstance,
   * so we access them through the factory's dependency manager.
   */
  private getHookRegistry() {
    const deps = this.sdk.getFactory().getDependencies();
    return deps.getHookTemplateRegistry();
  }

  /**
   * Register a hook template from a file
   */
  async registerHookTemplateFromFile(filePath: string, parameters?: Record<string, unknown>): Promise<HookTemplate> {
    return this.executeWithErrorHandling(async () => {
      const fullPath = resolve(process.cwd(), filePath);
      const parsed = await loadHookTemplateConfig(fullPath);
      let template = parsed.config as unknown as HookTemplate;

      if (parameters) {
        template = { ...template, ...parameters };
      }

      const registry = this.getHookRegistry();
      registry.register(template);

      this.logOperation(`Hook template registered: ${template.name}`);
      return template;
    }, "Register hook template");
  }

  /**
   * List all hook templates
   */
  async listHookTemplates(filter?: HookTemplateFilter): Promise<HookTemplateSummary[]> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.getHookRegistry();
      let summaries = registry.listSummaries();

      if (filter) {
        summaries = summaries.filter((s: HookTemplateSummary) => {
          if (filter.name && !s.name.includes(filter.name)) return false;
          if (filter.hookType && s.hookType !== filter.hookType) return false;
          if (filter.category && s.category !== filter.category) return false;
          if (filter.tags && s.tags) {
            if (!filter.tags.every(tag => s.tags?.includes(tag))) return false;
          }
          return true;
        });
      }

      return summaries;
    }, "List hook templates");
  }

  /**
   * Get a hook template by name
   */
  async getHookTemplate(name: string): Promise<HookTemplate> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.getHookRegistry();
      const template = registry.get(name);

      if (!template) {
        throw new CLINotFoundError(`Hook template not found: ${name}`, "HookTemplate", name);
      }

      return template;
    }, "Get hook template");
  }

  /**
   * Delete a hook template
   */
  async deleteHookTemplate(name: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.getHookRegistry();
      registry.unregister(name);
      this.logOperation(`Hook template deleted: ${name}`);
    }, "Delete hook template");
  }

  /**
   * Export a hook template as JSON string
   */
  async exportHookTemplate(name: string): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const registry = this.getHookRegistry();
      const json = registry.export(name);
      return json;
    }, "Export hook template");
  }
}