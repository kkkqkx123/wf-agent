/**
 * NodeTemplateRegistryAPI - Node Template Management API
 * Encapsulate NodeTemplateRegistry, provide CRUD operations for node templates.
 * Refactored version: Inherit GenericResourceAPI to improve code reusability and consistency.
 */

import type { NodeTemplate, Node } from "@wf-agent/types";
import { ValidationError, ConfigurationValidationError } from "@wf-agent/types";
import { NodeType } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err, getErrorMessage } from "@wf-agent/common-utils";
import { CrudResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
import type { Timestamp } from "@wf-agent/types";
import { validateNodeByType } from "../../../../graph/validation/node-validation/index.js";

/**
 * Node Template Filter
 */
export interface NodeTemplateFilter {
  /** Template name (fuzzy matching supported) */
  name?: string;
  /** Node type */
  nodeType?: string;
  /** categorization */
  category?: string;
  /** tagged array */
  tags?: string[];
}

/**
 * Summary of node templates
 */
export interface NodeTemplateSummary {
  /** Template name */
  name: string;
  /** Node type */
  type: NodeType;
  /** Node Description */
  description?: string;
  /** categorization */
  category?: string;
  /** tagged array */
  tags?: string[];
  /** Creation time */
  createdAt: Timestamp;
  /** update time */
  updatedAt: Timestamp;
}

/**
 * NodeTemplateRegistryAPI - Node Template Registry API
 *
 * Refactoring Description:
 * - Inherits GenericResourceAPI, reusing generic CRUD operations.
 * - Implement all abstract methods to adapt to NodeTemplateRegistry.
 * - Implement all abstract methods to adapt to NodeTemplateRegistry.
 * - Add caching, logging, validation and other enhancements.
 */
export class NodeRegistryAPI extends CrudResourceAPI<NodeTemplate, string, NodeTemplateFilter> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  /**
   * Get a single node template
   * @param id node template name
   * @returns node template, or null if it doesn't exist
   */
  protected async getResource(id: string): Promise<NodeTemplate | null> {
    const template = this.dependencies.getNodeTemplateRegistry().get(id);
    return template || null;
  }

  /**
   * Get all node templates
   * @returns array of node templates
   */
  protected async getAllResources(): Promise<NodeTemplate[]> {
    return this.dependencies.getNodeTemplateRegistry().list();
  }

  /**
   * Creating a node template
   * @param resource node template
   */
  protected async createResource(resource: NodeTemplate): Promise<void> {
    this.dependencies.getNodeTemplateRegistry().register(resource);
  }

  /**
   * Updating a node template
   * @param id node template name
   * @param updates Update content
   */
  protected async updateResource(id: string, updates: Partial<NodeTemplate>): Promise<void> {
    this.dependencies.getNodeTemplateRegistry().update(id, updates);
  }

  /**
   * Deleting a node template
   * @param id node template name
   */
  protected async deleteResource(id: string): Promise<void> {
    this.dependencies.getNodeTemplateRegistry().unregister(id);
  }

  /**
   * Applying Filters
   * @param resources array of node templates
   * @param filter Filtering criteria
   * @returns array of filtered node templates
   */
  protected override applyFilter(
    resources: NodeTemplate[],
    filter: NodeTemplateFilter,
  ): NodeTemplate[] {
    return resources.filter(template => {
      if (filter.name && !template.name.includes(filter.name)) {
        return false;
      }
      if (filter.nodeType && template.type !== filter.nodeType) {
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
   * Get a list of node template summaries
   * @param filter filter criteria
   * @returns array of node template summaries
   */
  async getTemplateSummaries(filter?: NodeTemplateFilter): Promise<NodeTemplateSummary[]> {
    const summaries = this.dependencies.getNodeTemplateRegistry().listSummaries();

    if (!filter) {
      return summaries;
    }

    return summaries.filter((summary: NodeTemplateSummary) => {
      if (filter.name && !summary.name.includes(filter.name)) {
        return false;
      }
      if (filter.nodeType && summary.type !== filter.nodeType) {
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
   * Get a list of node templates by type
   * @param type Node type
   * @returns Array of node templates
   */
  async getTemplatesByType(type: string): Promise<NodeTemplate[]> {
    return this.dependencies
      .getNodeTemplateRegistry()
      .listByType(type as import("@wf-agent/types").NodeType);
  }

  /**
   * Get list of node templates by tags
   * @param tags Array of tags
   * @returns array of node templates
   */
  async getTemplatesByTags(tags: string[]): Promise<NodeTemplate[]> {
    return this.dependencies.getNodeTemplateRegistry().listByTags(tags);
  }

  /**
   * Get a list of node templates by category
   * @param category Category
   * @returns array of node templates
   */
  async getTemplatesByCategory(category: string): Promise<NodeTemplate[]> {
    return this.dependencies.getNodeTemplateRegistry().listByCategory(category);
  }

  /**
   * Search Node Templates
   * @param keyword Search keyword
   * @returns array of node templates
   */
  async searchTemplates(keyword: string): Promise<NodeTemplate[]> {
    return this.dependencies.getNodeTemplateRegistry().search(keyword);
  }

  /**
   * Validate node templates (no side effects)
   * @param template node template
   * @returns Validation results
   */
  async validateTemplate(template: NodeTemplate): Promise<Result<NodeTemplate, ValidationError[]>> {
    const errors: ValidationError[] = [];

    // Validating Required Fields
    if (!template.name || typeof template.name !== "string") {
      errors.push(
        new ConfigurationValidationError("Node template name is required and must be a string", {
          configType: "node",
          configPath: "template.name",
          field: "name",
        }),
      );
    }

    if (
      !template.type ||
      ![
        "START",
        "END",
        "VARIABLE",
        "FORK",
        "JOIN",
        "SUBGRAPH",
        "SCRIPT",
        "LLM",
        "ADD_TOOL",
        "USER_INTERACTION",
        "ROUTE",
        "CONTEXT_PROCESSOR",
        "LOOP_START",
        "LOOP_END",
        "START_FROM_TRIGGER",
        "CONTINUE_FROM_TRIGGER",
      ].includes(template.type)
    ) {
      errors.push(
        new ConfigurationValidationError(`Invalid node type: ${template.type}`, {
          configType: "node",
          configPath: "template.type",
          field: "type",
        }),
      );
    }

    if (!template.config) {
      errors.push(
        new ConfigurationValidationError("Node template config is required", {
          configType: "node",
          configPath: "template.config",
          field: "config",
        }),
      );
    }

    // If there is an error, return directly
    if (errors.length > 0) {
      return err(errors);
    }

    // Verify node configurations using existing validation functions
    const mockNode = {
      id: "validation",
      type: template.type,
      name: template.name,
      config: template.config,
      outgoingEdgeIds: [],
      incomingEdgeIds: [],
    } as Node;

    try {
      validateNodeByType(mockNode);
      return ok(template);
    } catch (error) {
      if (error instanceof ValidationError) {
        errors.push(
          new ConfigurationValidationError(
            `Invalid node configuration for template '${template.name}': ${error.message}`,
            {
              configType: "node",
              configPath: "template.config",
              field: "config",
            },
          ),
        );
      } else {
        errors.push(
          new ConfigurationValidationError(getErrorMessage(error), {
            configType: "node",
            configPath: "template.config",
            field: "config",
          }),
        );
      }
      return err(errors);
    }
  }

  /**
   * Exporting node templates
   * @param name Node template name
   * @returns JSON string
   */
  async exportTemplate(name: string): Promise<string> {
    return this.dependencies.getNodeTemplateRegistry().export(name);
  }

  /**
   * Importing node templates
   * @param json JSON string
   * @returns node template name
   */
  async importTemplate(json: string): Promise<string> {
    const name = this.dependencies.getNodeTemplateRegistry().import(json);
    return name;
  }
}
