/**
 * Agent Hook Template Registry API
 * Provides a user-friendly API interface for managing agent hook templates.
 * Follows the same pattern as Workflow's HookTemplateRegistryAPI, using
 * the shared HookTemplate type directly.
 */

import type { Timestamp } from "@wf-agent/types";
import type { HookTemplate } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import { validateHookTemplate } from "../../shared/validation/hook-template-validator.js";

/**
 * Agent Hook Template Filter
 */
export interface AgentHookTemplateFilter {
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
 * Agent Hook Template Summary
 */
export interface AgentHookTemplateSummary {
  /** Template ID/name */
  id: string;
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
 * Agent Hook Template Registry API
 * Manages hook templates using the shared HookTemplate type directly.
 * Agent-specific metadata (category, tags, enabled) is stored in HookTemplate.metadata.
 */
export class AgentHookTemplateRegistryAPI extends SimplifiedCrudResourceAPI<
  HookTemplate,
  string,
  AgentHookTemplateFilter
> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  /**
   * Get a single hook template by ID
   * @param id Template ID/name
   * @returns The hook template; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<HookTemplate | null> {
    const template = this.dependencies.getHookTemplateRegistry().get(id);
    return template ?? null;
  }

  /**
   * Get all hook templates
   * @returns Array of all hook templates
   */
  protected async getAllResources(): Promise<HookTemplate[]> {
    return this.dependencies.getHookTemplateRegistry().list();
  }

  /**
   * Create a new hook template
   * @param resource Template to create
   */
  protected async createResource(resource: HookTemplate): Promise<void> {
    this.dependencies.getHookTemplateRegistry().register(resource);
  }

  /**
   * Update an existing hook template
   * @param id Template ID
   * @param updates Updated fields
   */
  protected async updateResource(
    id: string,
    updates: Partial<HookTemplate>
  ): Promise<void> {
    this.dependencies.getHookTemplateRegistry().update(id, updates);
  }

  /**
   * Delete a hook template
   * @param id Template ID to delete
   */
  protected async deleteResource(id: string): Promise<void> {
    this.dependencies.getHookTemplateRegistry().unregister(id);
  }

  /**
   * Apply filters to hook templates
   */
  protected override applyFilter(
    resources: HookTemplate[],
    filter: AgentHookTemplateFilter,
  ): HookTemplate[] {
    return resources.filter(template => {
      if (filter.name && !template.name.toLowerCase().includes(filter.name.toLowerCase())) {
        return false;
      }
      if (filter.hookType && template.hook.hookType !== filter.hookType) {
        return false;
      }
      const metadata = template.metadata ?? {};
      if (filter.category && metadata["category"] !== filter.category) {
        return false;
      }
      if (filter.tags && filter.tags.length > 0) {
        const tags = metadata["tags"] as string[] | undefined;
        if (!tags || !filter.tags.some((tag) => tags.includes(tag))) {
          return false;
        }
      }
      return true;
    });
  }

  /**
   * Query hook templates with filters
   * @param filter Query filter
   * @returns Array of matching hook templates
   */
  async query(filter: AgentHookTemplateFilter): Promise<HookTemplate[]> {
    return this.applyFilter(await this.getAllResources(), filter);
  }

  /**
   * Get hook templates by hook type
   * @param hookType Hook type to filter by
   * @returns Array of templates matching the type
   */
  async queryByHookType(hookType: string): Promise<HookTemplate[]> {
    return this.query({ hookType });
  }

  /**
   * Get hook templates by category
   * @param category Category to filter by
   * @returns Array of templates in the category
   */
  async queryByCategory(category: string): Promise<HookTemplate[]> {
    return this.query({ category });
  }

  /**
   * Get hook templates by tags
   * @param tags Tags to filter by
   * @returns Array of templates with any of the tags
   */
  async queryByTags(tags: string[]): Promise<HookTemplate[]> {
    return this.query({ tags });
  }

  /**
   * Get templates for a specific hook event
   * @param hookType Hook type
   * @returns Array of templates for that hook type
   */
  async getTemplatesForHook(hookType: string): Promise<HookTemplate[]> {
    return this.queryByHookType(hookType);
  }

  /**
   * Get hook template summaries.
   * @param filter - Optional filter criteria
   * @returns Array of hook template summaries
   */
  async getTemplateSummaries(filter?: AgentHookTemplateFilter): Promise<AgentHookTemplateSummary[]> {
    const summaries = this.dependencies.getHookTemplateRegistry().listSummaries();
    const mapped = summaries.map((s) => {
      const metadata = (s as unknown as Record<string, unknown>)["metadata"] as Record<string, unknown> | undefined;
      return {
        id: s.name,
        name: s.name,
        hookType: s.hookType,
        description: s.description,
        category: metadata?.["category"] as string | undefined,
        tags: metadata?.["tags"] as string[] | undefined,
        createdAt: s.createdAt,
        updatedAt: s.updatedAt,
      };
    });

    if (!filter) {
      return mapped;
    }

    return mapped.filter((summary) => {
      if (filter.name && !summary.name.toLowerCase().includes(filter.name.toLowerCase())) {
        return false;
      }
      if (filter.hookType && summary.hookType !== filter.hookType) {
        return false;
      }
      if (filter.category && summary.category !== filter.category) {
        return false;
      }
      if (filter.tags && filter.tags.length > 0) {
        if (!summary.tags || !filter.tags.some((tag) => summary.tags!.includes(tag))) {
          return false;
        }
      }
      return true;
    });
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
   * @param template - Hook template to validate
   * @returns Validation result
   */
  async validateTemplate(template: HookTemplate): Promise<Result<HookTemplate, ValidationError[]>> {
    return validateHookTemplate(template);
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