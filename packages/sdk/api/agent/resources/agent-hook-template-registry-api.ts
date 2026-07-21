/**
 * Agent Hook Template Registry API
 * Provides a user-friendly API interface for managing agent hook templates.
 * Follows the same pattern as Workflow's HookTemplateRegistryAPI.
 */

import type { Timestamp } from "@wf-agent/types";
import type { AgentHookType } from "@wf-agent/types";
import type { HookTemplate } from "@wf-agent/types";
import { ValidationError, ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";

/**
 * Agent Hook Template Filter
 */
export interface AgentHookTemplateFilter {
  /** Template name (fuzzy matching supported) */
  name?: string;
  /** Hook type filter */
  hookType?: AgentHookType;
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
  hookType: AgentHookType;
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
 * Agent Hook Template Definition
 * Wraps the underlying HookTemplate with agent-specific fields.
 * The `id` field is mapped from the underlying HookTemplate's `name`.
 */
export interface AgentHookTemplate {
  /** Template ID */
  id: string;
  /** Template name */
  name: string;
  /** Hook type */
  hookType: AgentHookType;
  /** Hook condition expression */
  condition?: string;
  /** Event name for identification */
  eventName: string;
  /** Event payload template */
  eventPayload?: Record<string, unknown>;
  /** Whether to create checkpoint */
  createCheckpoint?: boolean;
  /** Checkpoint description */
  checkpointDescription?: string;
  /** Priority weight */
  weight?: number;
  /** Template description */
  description?: string;
  /** Category for organization */
  category?: string;
  /** Tags for searching */
  tags?: string[];
  /** Creation time */
  createdAt: Timestamp;
  /** Update time */
  updatedAt: Timestamp;
  /** Whether template is enabled by default */
  enabled: boolean;
}

/**
 * Convert a HookTemplate from the shared registry to an AgentHookTemplate.
 * The shared HookTemplate stores hook config in a `hook` object, while
 * AgentHookTemplate flattens those fields at the top level.
 */
function toAgentHookTemplate(template: HookTemplate): AgentHookTemplate {
  const metadata = template.metadata ?? {};
  return {
    id: template.name,
    name: template.name,
    hookType: (template.hook?.hookType ?? "unknown") as AgentHookType,
    condition: template.hook?.condition as string | undefined,
    eventName: (template.hook?.eventName as string) ?? "",
    eventPayload: template.hook?.eventPayload as Record<string, unknown> | undefined,
    createCheckpoint: template.hook?.createCheckpoint,
    checkpointDescription: template.hook?.checkpointDescription,
    weight: (metadata as Record<string, unknown>)["weight"] as number | undefined,
    description: template.description,
    category: (metadata as Record<string, unknown>)["category"] as string | undefined,
    tags: (metadata as Record<string, unknown>)["tags"] as string[] | undefined,
    createdAt: template.createdAt,
    updatedAt: template.updatedAt,
    enabled: metadata["enabled"] !== false,
  };
}

/**
 * Agent Hook Template Registry API
 */
export class AgentHookTemplateRegistryAPI extends SimplifiedCrudResourceAPI<
  AgentHookTemplate,
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
  protected async getResource(id: string): Promise<AgentHookTemplate | null> {
    const template = this.dependencies.getHookTemplateRegistry().get(id);
    if (!template) return null;
    return toAgentHookTemplate(template);
  }

  /**
   * Get all hook templates
   * @returns Array of all hook templates
   */
  protected async getAllResources(): Promise<AgentHookTemplate[]> {
    const templates = this.dependencies.getHookTemplateRegistry().list();
    return templates.map(toAgentHookTemplate);
  }

  /**
   * Create a new hook template
   * @param resource Template to create
   */
  protected async createResource(resource: AgentHookTemplate): Promise<void> {
    this.dependencies.getHookTemplateRegistry().register(resource as unknown as HookTemplate);
  }

  /**
   * Update an existing hook template
   * @param id Template ID
   * @param updates Updated fields
   */
  protected async updateResource(
    id: string,
    updates: Partial<AgentHookTemplate>
  ): Promise<void> {
    this.dependencies.getHookTemplateRegistry().update(id, updates as unknown as Partial<HookTemplate>);
  }

  /**
   * Delete a hook template
   * @param id Template ID to delete
   */
  protected async deleteResource(id: string): Promise<void> {
    this.dependencies.getHookTemplateRegistry().unregister(id);
  }

  /**
   * Query hook templates with filters
   * @param filter Query filter
   * @returns Array of matching hook templates
   */
  async query(filter: AgentHookTemplateFilter): Promise<AgentHookTemplate[]> {
    const allTemplates = await this.getAllResources();

    let results = allTemplates;

    if (filter.name) {
      const nameLower = filter.name.toLowerCase();
      results = results.filter((t) => t.name.toLowerCase().includes(nameLower));
    }

    if (filter.hookType) {
      results = results.filter((t) => t.hookType === filter.hookType);
    }

    if (filter.category) {
      results = results.filter((t) => t.category === filter.category);
    }

    if (filter.tags && filter.tags.length > 0) {
      results = results.filter((t) =>
        t.tags ? filter.tags!.some((tag) => t.tags!.includes(tag)) : false
      );
    }

    return results;
  }

  /**
   * Get hook templates by hook type
   * @param hookType Hook type to filter by
   * @returns Array of templates matching the type
   */
  async queryByHookType(hookType: AgentHookType): Promise<AgentHookTemplate[]> {
    return this.query({ hookType });
  }

  /**
   * Get hook templates by category
   * @param category Category to filter by
   * @returns Array of templates in the category
   */
  async queryByCategory(category: string): Promise<AgentHookTemplate[]> {
    return this.query({ category });
  }

  /**
   * Get hook templates by tags
   * @param tags Tags to filter by
   * @returns Array of templates with any of the tags
   */
  async queryByTags(tags: string[]): Promise<AgentHookTemplate[]> {
    return this.query({ tags });
  }

  /**
   * Get templates for a specific hook event
   * @param hookType Hook type
   * @returns Array of templates for that hook type
   */
  async getTemplatesForHook(hookType: AgentHookType): Promise<AgentHookTemplate[]> {
    return this.queryByHookType(hookType);
  }

  /**
   * Get hook template summaries.
   * @param filter - Optional filter criteria
   * @returns Array of hook template summaries
   */
  async getTemplateSummaries(filter?: AgentHookTemplateFilter): Promise<AgentHookTemplateSummary[]> {
    const summaries = this.dependencies.getHookTemplateRegistry().listSummaries();

    if (!filter) {
      return summaries as AgentHookTemplateSummary[];
    }

    return (summaries as AgentHookTemplateSummary[]).filter((summary) => {
      if (filter.name && !summary.name.toLowerCase().includes(filter.name.toLowerCase())) {
        return false;
      }
      if (filter.hookType && summary.hookType !== filter.hookType) {
        return false;
      }
      if (filter.category && summary.category !== filter.category) {
        return false;
      }
      if (filter.tags && summary.tags) {
        if (!filter.tags.every((tag) => summary.tags!.includes(tag))) {
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
  async searchTemplates(keyword: string): Promise<AgentHookTemplate[]> {
    const templates = this.dependencies.getHookTemplateRegistry().search(keyword);
    return templates.map(toAgentHookTemplate);
  }

  /**
   * Validate a hook template (no side effects).
   * @param template - Hook template to validate
   * @returns Validation result
   */
  async validateTemplate(template: AgentHookTemplate): Promise<Result<AgentHookTemplate, ValidationError[]>> {
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

    if (!template.hookType) {
      errors.push(
        new ConfigurationValidationError("Hook template hookType is required", {
          configType: "hook_template",
          configPath: "template.hookType",
          field: "hookType",
        }),
      );
    }

    if (!template.eventName) {
      errors.push(
        new ConfigurationValidationError("Hook template eventName is required", {
          configType: "hook_template",
          configPath: "template.eventName",
          field: "eventName",
        }),
      );
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