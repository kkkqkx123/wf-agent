/**
 * Agent Template Registry API
 * Provides a user-friendly API interface for managing complete agent definition templates.
 * Allows users to query and manage reusable agent configurations.
 */

import type { Timestamp, AgentLoopDefinition } from "@wf-agent/types";
import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { AgentTemplateRegistry } from "../../../agent/registry/agent-template-registry.js";
import { now } from "@wf-agent/common-utils";

/**
 * Agent Template Filter
 */
export interface AgentTemplateFilter {
  /** Template name (fuzzy matching supported) */
  name?: string;
  /** Category filter */
  category?: string;
  /** Tags filter */
  tags?: string[];
  /** Profile type filter */
  profileType?: string;
}

/**
 * Agent Template Summary
 */
export interface AgentTemplateSummary {
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
  /** LLM profile type (e.g., "gpt-4", "claude-3-opus") */
  profileType?: string;
  /** Number of times used */
  usageCount?: number;
  /** Creation time */
  createdAt: Timestamp;
  /** Update time */
  updatedAt: Timestamp;
}

/**
 * Agent Template Definition
 */
export interface AgentTemplate extends AgentLoopDefinition {
  /** Template ID (overrides AgentLoopDefinition's id) */
  id: string;
  /** Template name (for display) */
  templateName: string;
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
 * Agent Template Registry API
 */
export class AgentTemplateRegistryAPI extends SimplifiedCrudResourceAPI<
  AgentTemplate,
  string,
  AgentTemplateFilter
> {
  private registry: AgentTemplateRegistry;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.registry = dependencies.getAgentTemplateRegistry();
  }

  /**
   * Get a single agent template by ID
   * @param id Template ID
   * @returns The agent template; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<AgentTemplate | null> {
    const template = this.registry.get(id);
    return template ?? null;
  }

  /**
   * Get all agent templates
   * @returns Array of all agent templates
   */
  protected async getAllResources(): Promise<AgentTemplate[]> {
    return this.registry.getAll();
  }

  /**
   * Create a new agent template
   * @param resource Template to create
   */
  protected async createResource(resource: AgentTemplate): Promise<void> {
    this.registry.register(resource);
  }

  /**
   * Update an existing agent template
   * @param id Template ID
   * @param updates Updated fields
   */
  protected async updateResource(
    id: string,
    updates: Partial<AgentTemplate>
  ): Promise<void> {
    this.registry.update(id, updates);
  }

  /**
   * Delete an agent template
   * @param id Template ID to delete
   */
  protected async deleteResource(id: string): Promise<void> {
    this.registry.unregister(id);
  }

  /**
   * Query agent templates with filters
   * @param filter Query filter
   * @returns Array of matching agent templates
   */
  async query(filter: AgentTemplateFilter): Promise<AgentTemplate[]> {
    const allTemplates = await this.getAllResources();

    let results = allTemplates;

    if (filter.name) {
      const nameLower = filter.name.toLowerCase();
      results = results.filter((t) =>
        t.templateName.toLowerCase().includes(nameLower) ||
        (t.name && t.name.toLowerCase().includes(nameLower))
      );
    }

    if (filter.category) {
      results = results.filter((t) => t.templateCategory === filter.category);
    }

    if (filter.tags && filter.tags.length > 0) {
      results = results.filter((t) =>
        t.templateTags ? filter.tags!.some((tag) => t.templateTags!.includes(tag)) : false
      );
    }

    if (filter.profileType) {
      results = results.filter((t) => t.profileId === filter.profileType);
    }

    return results;
  }

  /**
   * Get agent templates by category
   * @param category Category to filter by
   * @returns Array of templates in the category
   */
  async queryByCategory(category: string): Promise<AgentTemplate[]> {
    return this.query({ category });
  }

  /**
   * Get agent templates by tags
   * @param tags Tags to filter by
   * @returns Array of templates with any of the tags
   */
  async queryByTags(tags: string[]): Promise<AgentTemplate[]> {
    return this.query({ tags });
  }

  /**
   * Get agent templates by profile type
   * @param profileType Profile/model type to filter by
   * @returns Array of templates using the profile type
   */
  async queryByProfileType(profileType: string): Promise<AgentTemplate[]> {
    return this.query({ profileType });
  }

  /**
   * Get agent templates by author
   * @param author Author to filter by
   * @returns Array of templates by the author
   */
  async queryByAuthor(author: string): Promise<AgentTemplate[]> {
    const allTemplates = await this.getAllResources();
    return allTemplates.filter((t) => {
      const metadata = t.metadata;
      if (!metadata) return false;
      return (metadata as Record<string, unknown>)["author"] === author;
    });
  }

  /**
   * Get featured/popular agent templates
   * @returns Array of most used agent templates
   */
  async getFeaturedTemplates(): Promise<AgentTemplate[]> {
    const allTemplates = await this.getAllResources();
    return allTemplates
      .filter((t) => t.isPublic && t.enabled !== false)
      .sort((a, b) => (b.usageCount || 0) - (a.usageCount || 0))
      .slice(0, 10);
  }

  /**
   * Get templates by category and sort by popularity
   * @param category Category to filter by
   * @param limit Number of results to return
   * @returns Array of popular templates in category
   */
  async getPopularInCategory(category: string, limit: number = 10): Promise<AgentTemplate[]> {
    const templates = await this.queryByCategory(category);
    return templates
      .filter((t) => t.enabled !== false)
      .sort((a, b) => (b.usageCount || 0) - (a.usageCount || 0))
      .slice(0, limit);
  }

  /**
   * Clone a template to create a new agent configuration
   * @param templateId Template ID to clone
   * @param newName Name for the new configuration
   * @returns The cloned agent definition
   */
  async cloneTemplate(templateId: string, newName: string): Promise<AgentLoopDefinition | null> {
    const template = this.registry.get(templateId);
    if (!template) return null;

    // Create a new definition based on the template
    const cloned: AgentLoopDefinition = {
      id: `cloned-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      name: newName,
      version: template.version,
      description: template.description ? `Clone of ${template.description}` : undefined,
      profileId: template.profileId,
      systemPrompt: template.systemPrompt,
      systemPromptTemplateId: template.systemPromptTemplateId,
      systemPromptTemplateVariables: template.systemPromptTemplateVariables,
      maxIterations: template.maxIterations,
      initialMessages: template.initialMessages,
      availableTools: template.availableTools,
      stream: template.stream,
      hooks: template.hooks ? [...template.hooks] : undefined,
      triggers: template.triggers ? [...template.triggers] : undefined,
      dynamicContext: template.dynamicContext,
      checkpoint: template.checkpoint,
      metadata: template.metadata ? { ...template.metadata } : undefined,
      createdAt: now(),
    };

    return cloned;
  }

  /**
   * Increment the usage count for a template
   * @param templateId Template ID
   */
  async incrementUsageCount(templateId: string): Promise<void> {
    this.registry.incrementUsageCount(templateId);
  }
}