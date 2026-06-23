/**
 * Agent Hook Template Registry API
 * Provides a user-friendly API interface for managing agent hook templates.
 * Follows the same pattern as Workflow's HookTemplateRegistryAPI.
 */

import type { Timestamp } from "@wf-agent/types";
import type { AgentHookType } from "@wf-agent/types";
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
 * Agent Hook Template Registry API
 */
export class AgentHookTemplateRegistryAPI extends SimplifiedCrudResourceAPI<
  AgentHookTemplate,
  string,
  AgentHookTemplateFilter
> {
  constructor(dependencies: APIDependencyManager) {
    super();
    // Store dependencies if needed for future implementation
    void dependencies;
  }

  /**
   * Get a single hook template by ID
   * @param id Template ID/name
   * @returns The hook template; returns null if it does not exist
   */
  protected async getResource(): Promise<AgentHookTemplate | null> {
    // In a full implementation, this would retrieve from a template registry
    // For now, returns null as placeholder for future implementation
    return null;
  }

  /**
   * Get all hook templates
   * @returns Array of all hook templates
   */
  protected async getAllResources(): Promise<AgentHookTemplate[]> {
    // In a full implementation, this would retrieve all templates from registry
    return [];
  }

  /**
   * Create a new hook template
   * @param resource Template to create
   */
  protected async createResource(resource: AgentHookTemplate): Promise<void> {
    void resource; // Suppress unused parameter warning
    // In a full implementation, this would persist to storage
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
    void id;
    void updates; // Suppress unused parameter warning
    // In a full implementation, this would persist to storage
  }

  /**
   * Delete a hook template
   * @param id Template ID to delete
   */
  protected async deleteResource(id: string): Promise<void> {
    void id; // Suppress unused parameter warning
    // In a full implementation, this would delete from storage
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
}
