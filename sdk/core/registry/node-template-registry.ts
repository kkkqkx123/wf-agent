/**
 * Node Registry
 * Responsible for the registration, querying, and management of node templates
 *
 * This module only exports class definitions; instances are managed uniformly through the SingletonRegistry.
 *
 */

import type { NodeTemplate, NodeTemplateSummary } from "@wf-agent/types";
import type { Node } from "@wf-agent/types";
import { NodeType } from "@wf-agent/types";
import {
  ValidationError,
  ConfigurationValidationError,
  NodeTemplateNotFoundError,
} from "@wf-agent/types";
import { validateNodeByType } from "../../workflow/validation/node-validation/index.js";
import { getErrorMessage, now } from "@wf-agent/common-utils";

/**
 * Node Registry Class
 */
class NodeTemplateRegistry {
  private templates: Map<string, NodeTemplate> = new Map();

  /**
   * Register Node Template
   * @param template: The node template
   * @throws ValidationError: If the node configuration is invalid or the name already exists
   */
  register(template: NodeTemplate): void {
    // Verify node configuration
    this.validateTemplate(template);

    // Check if the name already exists.
    if (this.templates.has(template.name)) {
      throw new ConfigurationValidationError(
        `Node template with name '${template.name}' already exists`,
        {
          configType: "node",
          configPath: "template.name",
        },
      );
    }

    // Register Node Template
    this.templates.set(template.name, template);
  }

  /**
   * Batch registration of node templates
   * @param templates: Array of node templates
   */
  registerBatch(templates: NodeTemplate[]): void {
    for (const template of templates) {
      this.register(template);
    }
  }

  /**
   * Get node template
   * @param name Name of the node template
   * @returns The node template; returns undefined if it does not exist
   */
  get(name: string): NodeTemplate | undefined {
    return this.templates.get(name);
  }

  /**
   * Check if the node template exists
   * @param name Name of the node template
   * @returns Whether it exists
   */
  has(name: string): boolean {
    return this.templates.has(name);
  }

  /**
   * Update Node Template
   * @param name Name of the node template
   * @param updates Updates to be applied
   * @throws NotFoundError If the node template does not exist
   * @throws ValidationError If the updated configuration is invalid
   */
  update(name: string, updates: Partial<NodeTemplate>): void {
    const template = this.templates.get(name);
    if (!template) {
      throw new NodeTemplateNotFoundError(`Node template '${name}' not found`, name);
    }

    // Create the updated template.
    const updatedTemplate: NodeTemplate = {
      ...template,
      ...updates,
      name: template.name, // The name cannot be changed.
      updatedAt: now(),
    };

    // Verify the updated template.
    this.validateTemplate(updatedTemplate);

    // Update the template
    this.templates.set(name, updatedTemplate);
  }

  /**
   * Delete node template
   * @param name Name of the node template
   * @throws NotFoundError If the node template does not exist
   */
  unregister(name: string): void {
    if (!this.templates.has(name)) {
      throw new NodeTemplateNotFoundError(`Node template '${name}' not found`, name);
    }
    this.templates.delete(name);
  }

  /**
   * Batch delete node templates
   * @param names Array of node template names
   */
  unregisterBatch(names: string[]): void {
    for (const name of names) {
      this.unregister(name);
    }
  }

  /**
   * List all node templates
   * @returns Array of node templates
   */
  list(): NodeTemplate[] {
    return Array.from(this.templates.values());
  }

  /**
   * List all node template summaries
   * @returns Array of node template summaries
   */
  listSummaries(): NodeTemplateSummary[] {
    return this.list().map(template => {
      const summary: NodeTemplateSummary = {
        name: template.name,
        type: template.type,
        description: template.description,
        createdAt: template.createdAt,
        updatedAt: template.updatedAt,
      };

      if (template.metadata?.["category"]) {
        summary.category = template.metadata["category"] as string;
      }
      if (template.metadata?.["tags"]) {
        summary.tags = template.metadata["tags"] as string[];
      }

      return summary;
    });
  }

  /**
   * List node templates by type
   * @param type: Node type
   * @returns: Array of node templates
   */
  listByType(type: NodeType): NodeTemplate[] {
    return this.list().filter(template => template.type === type);
  }

  /**
   * List node templates by category
   * @param category Category
   * @returns Array of node templates
   */
  listByCategory(category: string): NodeTemplate[] {
    return this.list().filter(template => template.metadata?.["category"] === category);
  }

  /**
   * List node templates by tag
   * @param tags An array of tags
   * @returns An array of node templates
   */
  listByTags(tags: string[]): NodeTemplate[] {
    return this.list().filter(template => {
      const templateTags = (template.metadata?.["tags"] as string[]) || [];
      return tags.every(tag => templateTags.includes(tag));
    });
  }

  /**
   * Clear all node templates.
   */
  clear(): void {
    this.templates.clear();
  }

  /**
   * Get the number of node templates
   * @returns The number of node templates
   */
  size(): number {
    return this.templates.size;
  }

  /**
   * Search for node templates
   * @param keyword: Search keyword
   * @returns: Array of matching node templates
   */
  search(keyword: string): NodeTemplate[] {
    const lowerKeyword = keyword.toLowerCase();
    return this.list().filter(template => {
      return (
        template.name.toLowerCase().includes(lowerKeyword) ||
        template.description?.toLowerCase().includes(lowerKeyword) ||
        (template.metadata?.["tags"] as string[])?.some((tag: string) =>
          tag.toLowerCase().includes(lowerKeyword),
        ) ||
        (template.metadata?.["category"] as string)?.toLowerCase().includes(lowerKeyword)
      );
    });
  }

  /**
   * Verify node template
   * @param template: The node template
   * @throws ValidationError: If the node configuration is invalid
   */
  private validateTemplate(template: NodeTemplate): void {
    // Verify required fields
    if (!template.name || typeof template.name !== "string") {
      throw new ConfigurationValidationError(
        "Node template name is required and must be a string",
        {
          configType: "node",
          configPath: "template.name",
        },
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
      throw new ConfigurationValidationError(`Invalid node type: ${template.type}`, {
        configType: "node",
        configPath: "template.type",
      });
    }

    if (!template.config) {
      throw new ConfigurationValidationError("Node template config is required", {
        configType: "node",
        configPath: "template.config",
      });
    }

    // Verify node configuration using existing verification functions
    // Use type assertions because this is a temporary verification object
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
    } catch (error) {
      if (error instanceof ValidationError) {
        throw new ConfigurationValidationError(
          `Invalid node configuration for template '${template.name}': ${error.message}`,
          {
            configType: "node",
            configPath: "template.config",
          },
        );
      }
      throw error;
    }
  }

  /**
   * Export the node template as a JSON string.
   * @param name: The name of the node template
   * @returns: A JSON string
   * @throws: NotFoundError if the node template does not exist
   */
  export(name: string): string {
    const template = this.templates.get(name);
    if (!template) {
      throw new NodeTemplateNotFoundError(`Node template '${name}' not found`, name);
    }
    return JSON.stringify(template, null, 2);
  }

  /**
   * Import node templates from a JSON string
   * @param json JSON string
   * @returns Node template name
   * @throws ValidationError If the JSON is invalid or the node configuration is incorrect
   */
  import(json: string): string {
    try {
      const template = JSON.parse(json) as NodeTemplate;
      this.register(template);
      return template.name;
    } catch (error) {
      if (error instanceof ValidationError) {
        throw error;
      }
      throw new ConfigurationValidationError(
        `Failed to import node template: ${getErrorMessage(error)}`,
        {
          configType: "node",
          configPath: "json",
        },
      );
    }
  }
}

/**
 * Export the NodeTemplateRegistry class
 */
export { NodeTemplateRegistry };
