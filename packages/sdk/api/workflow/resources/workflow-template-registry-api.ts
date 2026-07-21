/**
 * WorkflowTemplateRegistryAPI - Workflow Template Registry API
 *
 * Provides a user-friendly API interface for managing complete workflow definition templates.
 * Mirrors the AgentTemplateRegistryAPI pattern for consistency across the SDK.
 * Allows users to query, clone, and manage reusable workflow configurations.
 */

import type { Timestamp, WorkflowTemplate } from "@wf-agent/types";
import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import { now } from "@wf-agent/common-utils";

/**
 * Workflow Template Filter
 */
export interface WorkflowTemplateFilter {
  /** Template name (fuzzy matching supported) */
  name?: string;
  /** Category filter */
  category?: string;
  /** Tags filter */
  tags?: string[];
  /** Author filter */
  author?: string;
}

/**
 * Workflow Template Summary
 */
export interface WorkflowTemplateSummary {
  /** Template ID */
  id: string;
  /** Template name */
  name: string;
  /** Template description */
  description?: string;
  /** Category for organization */
  category?: string;
  /** Tags for searching */
  tags?: string[];
  /** Author */
  author?: string;
  /** Whether this is a public template */
  isPublic?: boolean;
  /** Number of times used */
  usageCount?: number;
  /** Creation time */
  createdAt: Timestamp;
  /** Update time */
  updatedAt: Timestamp;
}

/**
 * Extended workflow template with template-specific metadata
 */
export interface WorkflowTemplateDefinition extends WorkflowTemplate {
  /** Template category (for organization) */
  templateCategory?: string;
  /** Template tags (for searching) */
  templateTags?: string[];
  /** Whether this is a public template */
  isPublic?: boolean;
  /** Number of times used */
  usageCount?: number;
  /** Whether template is enabled */
  enabled?: boolean;
}

/**
 * WorkflowTemplateRegistryAPI - Workflow Template Registry API
 */
export class WorkflowTemplateRegistryAPI extends SimplifiedCrudResourceAPI<
  WorkflowTemplateDefinition,
  string,
  WorkflowTemplateFilter
> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  /**
   * Get a single workflow template by ID
   * @param id Template ID
   * @returns The workflow template; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<WorkflowTemplateDefinition | null> {
    const template = this.dependencies.getWorkflowRegistry().get(id);
    if (!template) return null;
    return this.toWorkflowTemplateDefinition(template);
  }

  /**
   * Get all workflow templates
   * @returns Array of all workflow templates
   */
  protected async getAllResources(): Promise<WorkflowTemplateDefinition[]> {
    const summaries = await this.dependencies.getWorkflowRegistry().list();
    const templates: WorkflowTemplateDefinition[] = [];
    for (const summary of summaries) {
      const template = this.dependencies.getWorkflowRegistry().get(summary.id);
      if (template) {
        templates.push(this.toWorkflowTemplateDefinition(template));
      }
    }
    return templates;
  }

  /**
   * Create a new workflow template
   * @param resource Template to create
   */
  protected async createResource(resource: WorkflowTemplateDefinition): Promise<void> {
    await this.dependencies.getWorkflowRegistry().register(resource);
  }

  /**
   * Update an existing workflow template
   * @param id Template ID
   * @param updates Updated fields
   */
  protected async updateResource(
    id: string,
    updates: Partial<WorkflowTemplateDefinition>,
  ): Promise<void> {
    await this.dependencies.getWorkflowRegistry().update(id, updates);
  }

  /**
   * Delete a workflow template
   * @param id Template ID to delete
   */
  protected async deleteResource(id: string): Promise<void> {
    await this.dependencies.getWorkflowRegistry().unregister(id);
  }

  /**
   * Apply filters to workflow templates
   * @param resources Array of templates
   * @param filter Filter criteria
   * @returns Filtered array
   */
  protected override applyFilter(
    resources: WorkflowTemplateDefinition[],
    filter: WorkflowTemplateFilter,
  ): WorkflowTemplateDefinition[] {
    return resources.filter(template => {
      if (filter.name) {
        const nameLower = filter.name.toLowerCase();
        if (
          !template.name.toLowerCase().includes(nameLower) &&
          !(template as any).templateName?.toLowerCase().includes(nameLower)
        ) {
          return false;
        }
      }
      if (filter.category && template.templateCategory !== filter.category) {
        return false;
      }
      if (filter.tags && filter.tags.length > 0) {
        if (!template.templateTags) return false;
        if (!filter.tags.some(tag => template.templateTags!.includes(tag))) {
          return false;
        }
      }
      if (filter.author && (template as any).author !== filter.author) {
        return false;
      }
      return true;
    });
  }

  /**
   * Query workflow templates with filters
   * @param filter Query filter
   * @returns Array of matching workflow templates
   */
  async query(filter: WorkflowTemplateFilter): Promise<WorkflowTemplateDefinition[]> {
    return this.applyFilter(await this.getAllResources(), filter);
  }

  /**
   * Get workflow templates by category
   * @param category Category to filter by
   * @returns Array of templates in the category
   */
  async queryByCategory(category: string): Promise<WorkflowTemplateDefinition[]> {
    return this.query({ category });
  }

  /**
   * Get workflow templates by tags
   * @param tags Tags to filter by
   * @returns Array of templates with any of the tags
   */
  async queryByTags(tags: string[]): Promise<WorkflowTemplateDefinition[]> {
    return this.query({ tags });
  }

  /**
   * Get workflow templates by author
   * @param author Author to filter by
   * @returns Array of templates by the author
   */
  async queryByAuthor(author: string): Promise<WorkflowTemplateDefinition[]> {
    return this.query({ author });
  }

  /**
   * Get featured/popular workflow templates
   * @returns Array of most used workflow templates
   */
  async getFeaturedTemplates(): Promise<WorkflowTemplateDefinition[]> {
    const allTemplates = await this.getAllResources();
    return allTemplates
      .filter((t) => t.isPublic !== false && t.enabled !== false)
      .sort((a, b) => (b.usageCount || 0) - (a.usageCount || 0))
      .slice(0, 10);
  }

  /**
   * Get templates by category and sort by popularity
   * @param category Category to filter by
   * @param limit Number of results to return
   * @returns Array of popular templates in category
   */
  async getPopularInCategory(category: string, limit: number = 10): Promise<WorkflowTemplateDefinition[]> {
    const templates = await this.queryByCategory(category);
    return templates
      .filter((t) => t.enabled !== false)
      .sort((a, b) => (b.usageCount || 0) - (a.usageCount || 0))
      .slice(0, limit);
  }

  /**
   * Clone a template to create a new workflow configuration
   * @param templateId Template ID to clone
   * @param newName Name for the new workflow
   * @returns The cloned workflow template
   */
  async cloneTemplate(templateId: string, newName: string): Promise<WorkflowTemplate | null> {
    const template = this.dependencies.getWorkflowRegistry().get(templateId);
    if (!template) return null;

    const cloned: WorkflowTemplate = {
      ...template,
      id: `cloned-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      name: newName,
      description: template.description ? `Clone of ${template.description}` : undefined,
      createdAt: now(),
    };

    return cloned;
  }

  /**
   * Increment the usage count for a workflow template
   * @param templateId Template ID
   */
  async incrementUsageCount(templateId: string): Promise<void> {
    const template = this.dependencies.getWorkflowRegistry().get(templateId);
    if (template) {
      const metadata = (template as any).metadata ?? {};
      metadata.usageCount = (metadata.usageCount ?? 0) + 1;
      await this.dependencies.getWorkflowRegistry().update(templateId, { metadata } as any);
    }
  }

  /**
   * Convert a WorkflowTemplate to WorkflowTemplateDefinition
   * @param template Raw workflow template
   * @returns Extended workflow template definition
   */
  private toWorkflowTemplateDefinition(template: WorkflowTemplate): WorkflowTemplateDefinition {
    const metadata = (template as any).metadata ?? {};
    return {
      ...template,
      templateCategory: metadata.category,
      templateTags: metadata.tags,
      isPublic: metadata.isPublic !== false,
      usageCount: metadata.usageCount ?? 0,
      enabled: metadata.enabled !== false,
    };
  }
}