/**
 * HookTemplateRegistryAPI - Hook Template Management API
 * Encapsulate HookTemplateRegistry, provide CRUD operations for hook templates.
 */

import type { HookTemplate } from "@wf-agent/types";
import { ValidationError, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { Timestamp } from "@wf-agent/types";

/**
 * Hook Template Filter
 */
export interface HookTemplateFilter {
  /** Template name (fuzzy matching supported) */
  name?: string;
  /** Hook type filter */
  hookType?: string;
  /** Category filter */
  category?: string;
  /** Tags filter */
  tags?: string[];
}

/**
 * Summary of hook templates
 */
export interface HookTemplateSummary {
  /** Template name */
  name: string;
  /** Hook type */
  hookType: string;
  /** Template description */
  description?: string;
  /** Category */
  category?: string;
  /** Tags */
  tags?: string[];
  /** Creation time */
  createdAt: Timestamp;
  /** Update time */
  updatedAt: Timestamp;
}

/**
 * HookTemplateRegistryAPI - Hook Template Registry API
 */
export class HookRegistryAPI extends SimplifiedCrudResourceAPI<HookTemplate, string, HookTemplateFilter> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  /**
   * Get a single hook template
   * @param id - Hook template name
   * @returns Hook template, or null if it doesn't exist
   */
  protected async getResource(id: string): Promise<HookTemplate | null> {
    const template = this.dependencies.getHookTemplateRegistry().get(id);
    return template || null;
  }

  /**
   * Get all hook templates
   * @returns Array of hook templates
   */
  protected async getAllResources(): Promise<HookTemplate[]> {
    return this.dependencies.getHookTemplateRegistry().list();
  }

  /**
   * Create a hook template
   * @param resource - Hook template
   */
  protected async createResource(resource: HookTemplate): Promise<void> {
    this.dependencies.getHookTemplateRegistry().register(resource);
  }

  /**
   * Update a hook template
   * @param id - Hook template name
   * @param updates - Update content
   */
  protected async updateResource(id: string, updates: Partial<HookTemplate>): Promise<void> {
    this.dependencies.getHookTemplateRegistry().update(id, updates);
  }

  /**
   * Delete a hook template
   * @param id - Hook template name
   */
  protected async deleteResource(id: string): Promise<void> {
    this.dependencies.getHookTemplateRegistry().unregister(id);
  }

  /**
   * Apply filters
   * @param resources - Array of hook templates
   * @param filter - Filter criteria
   * @returns Filtered array of hook templates
   */
  protected override applyFilter(
    resources: HookTemplate[],
    filter: HookTemplateFilter,
  ): HookTemplate[] {
    return resources.filter(template => {
      if (filter.name && !template.name.includes(filter.name)) {
        return false;
      }
      if (filter.hookType && template.hook.hookType !== filter.hookType) {
        return false;
      }
      if (filter.category && template.metadata?.["category"] !== filter.category) {
        return false;
      }
      if (filter.tags && template.metadata?.["tags"]) {
        const tags = template.metadata["tags"] as string[];
        if (!filter.tags.every(tag => tags.includes(tag))) {
          return false;
        }
      }
      return true;
    });
  }

  /**
   * Get hook template summaries.
   * @param filter - Optional filter criteria
   * @returns Array of hook template summaries
   */
  async getTemplateSummaries(filter?: HookTemplateFilter): Promise<HookTemplateSummary[]> {
    const summaries = this.dependencies.getHookTemplateRegistry().listSummaries();

    if (!filter) {
      return summaries;
    }

    return summaries.filter((summary: HookTemplateSummary) => {
      if (filter.name && !summary.name.includes(filter.name)) {
        return false;
      }
      if (filter.hookType && summary.hookType !== filter.hookType) {
        return false;
      }
      if (filter.category && summary.category !== filter.category) {
        return false;
      }
      if (filter.tags && summary.tags) {
        if (!filter.tags.every(tag => summary.tags?.includes(tag))) {
          return false;
        }
      }
      return true;
    });
  }

  /**
   * Get hook templates by hook type.
   * @param hookType - Hook type
   * @returns Array of hook templates
   */
  async getTemplatesByHookType(hookType: string): Promise<HookTemplate[]> {
    return this.dependencies.getHookTemplateRegistry().listByHookType(hookType);
  }

  /**
   * Get hook templates by tags.
   * @param tags - Array of tags
   * @returns Array of hook templates
   */
  async getTemplatesByTags(tags: string[]): Promise<HookTemplate[]> {
    return this.dependencies.getHookTemplateRegistry().listByTags(tags);
  }

  /**
   * Get hook templates by category.
   * @param category - Category
   * @returns Array of hook templates
   */
  async getTemplatesByCategory(category: string): Promise<HookTemplate[]> {
    return this.dependencies.getHookTemplateRegistry().listByCategory(category);
  }

  /**
   * Search hook templates.
   * @param keyword - Search keyword
   * @returns Array of matching hook templates
   */
  async searchTemplates(keyword: string): Promise<HookTemplate[]> {
    return this.dependencies.getHookTemplateRegistry().search(keyword);
  }

  /**
   * Validate a hook template (no side effects).
   * @param template - Hook template
   * @returns Validation result
   */
  async validateTemplate(template: HookTemplate): Promise<Result<HookTemplate, ValidationError[]>> {
    const errors: ValidationError[] = [];

    if (!template.name || typeof template.name !== "string") {
      errors.push(
        new ConfigurationValidationError("Hook template name is required and must be a string", {
          configType: "hook_template",
          configPath: "template.name",
          field: "name",
        }),
      );
    }

    if (!template.hook) {
      errors.push(
        new ConfigurationValidationError("Hook template hook configuration is required", {
          configType: "hook_template",
          configPath: "template.hook",
          field: "hook",
        }),
      );
    } else {
      if (!template.hook.hookType) {
        errors.push(
          new ConfigurationValidationError("Hook template hook.hookType is required", {
            configType: "hook_template",
            configPath: "template.hook.hookType",
            field: "hookType",
          }),
        );
      }
      if (!template.hook.eventName) {
        errors.push(
          new ConfigurationValidationError("Hook template hook.eventName is required", {
            configType: "hook_template",
            configPath: "template.hook.eventName",
            field: "eventName",
          }),
        );
      }
    }

    if (errors.length > 0) {
      return err(errors);
    }

    return ok(template);
  }

  /**
   * Export a hook template as JSON.
   * @param name - Hook template name
   * @returns JSON string
   */
  async exportTemplate(name: string): Promise<string> {
    return this.dependencies.getHookTemplateRegistry().export(name);
  }

  /**
   * Import a hook template from JSON.
   * @param json - JSON string
   * @returns Hook template name
   */
  async importTemplate(json: string): Promise<string> {
    return this.dependencies.getHookTemplateRegistry().import(json);
  }
}
