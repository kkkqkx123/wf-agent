/**
 * Agent Trigger Template Registry API
 * Provides a user-friendly API interface for managing agent trigger templates.
 * Follows the same pattern as Workflow's TriggerTemplateRegistryAPI.
 */

import type { Timestamp } from "@wf-agent/types";
import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";

/**
 * Agent Trigger Template Filter
 */
export interface AgentTriggerTemplateFilter {
  /** Template name (fuzzy matching supported) */
  name?: string;
  /** Trigger type filter */
  triggerType?: string;
  /** Category filter */
  category?: string;
  /** Tags filter */
  tags?: string[];
}

/**
 * Agent Trigger Template Summary
 */
export interface AgentTriggerTemplateSummary {
  /** Template ID/name */
  id: string;
  /** Template name */
  name: string;
  /** Trigger type */
  triggerType: "event" | "condition" | "schedule";
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
 * Agent Trigger Template Definition
 */
export interface AgentTriggerTemplate {
  /** Template ID */
  id: string;
  /** Template name */
  name: string;
  /** Trigger type */
  type: "event" | "condition" | "schedule";
  /** Trigger condition expression */
  condition?: string;
  /** Event name (for event-based triggers) */
  eventName?: string;
  /** Default action type */
  actionType: "pause" | "stop" | "checkpoint" | "custom";
  /** Action configuration */
  actionConfig?: Record<string, unknown>;
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
  /** Whether template is enabled */
  enabled: boolean;
}

/**
 * Agent Trigger Template Registry API
 */
export class AgentTriggerTemplateRegistryAPI extends SimplifiedCrudResourceAPI<
  AgentTriggerTemplate,
  string,
  AgentTriggerTemplateFilter
> {
  constructor(dependencies: APIDependencyManager) {
    super();
    // Store dependencies if needed for future implementation
    void dependencies;
  }

  /**
   * Get a single trigger template by ID
   * @param id Template ID/name
   * @returns The trigger template; returns null if it does not exist
   */
  protected async getResource(): Promise<AgentTriggerTemplate | null> {
    // In a full implementation, this would retrieve from a template registry
    // For now, returns null as placeholder for future implementation
    return null;
  }

  /**
   * Get all trigger templates
   * @returns Array of all trigger templates
   */
  protected async getAllResources(): Promise<AgentTriggerTemplate[]> {
    // In a full implementation, this would retrieve all templates from registry
    return [];
  }

  /**
   * Create a new trigger template
   * @param resource Template to create
   */
  protected async createResource(resource: AgentTriggerTemplate): Promise<void> {
    void resource; // Suppress unused parameter warning
    // In a full implementation, this would persist to storage
  }

  /**
   * Update an existing trigger template
   * @param id Template ID
   * @param updates Updated fields
   */
  protected async updateResource(
    id: string,
    updates: Partial<AgentTriggerTemplate>
  ): Promise<void> {
    void id;
    void updates; // Suppress unused parameter warning
    // In a full implementation, this would persist to storage
  }

  /**
   * Delete a trigger template
   * @param id Template ID to delete
   */
  protected async deleteResource(id: string): Promise<void> {
    void id; // Suppress unused parameter warning
    // In a full implementation, this would delete from storage
  }

  /**
   * Query trigger templates with filters
   * @param filter Query filter
   * @returns Array of matching trigger templates
   */
  async query(filter: AgentTriggerTemplateFilter): Promise<AgentTriggerTemplate[]> {
    const allTemplates = await this.getAllResources();

    let results = allTemplates;

    if (filter.name) {
      const nameLower = filter.name.toLowerCase();
      results = results.filter((t) => t.name.toLowerCase().includes(nameLower));
    }

    if (filter.triggerType) {
      results = results.filter((t) => t.type === filter.triggerType);
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
   * Get trigger templates by type
   * @param triggerType Trigger type to filter by
   * @returns Array of templates matching the type
   */
  async queryByType(triggerType: "event" | "condition" | "schedule"): Promise<AgentTriggerTemplate[]> {
    return this.query({ triggerType });
  }

  /**
   * Get trigger templates by category
   * @param category Category to filter by
   * @returns Array of templates in the category
   */
  async queryByCategory(category: string): Promise<AgentTriggerTemplate[]> {
    return this.query({ category });
  }

  /**
   * Get trigger templates by tags
   * @param tags Tags to filter by
   * @returns Array of templates with any of the tags
   */
  async queryByTags(tags: string[]): Promise<AgentTriggerTemplate[]> {
    return this.query({ tags });
  }
}
