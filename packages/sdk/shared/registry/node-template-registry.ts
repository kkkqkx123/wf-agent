/**
 * Node Template Registry
 *
 * Responsible for the registration, querying, and management of node templates.
 * Implements standardized registry interfaces for consistency.
 *
 * This module only exports class definitions; instances are managed by the DI container as singletons.
 *
 * Interface Implementation:
 * - Registry<NodeTemplate>: Read operations
 * - MutableRegistry<NodeTemplate>: Write operations
 * - SearchableRegistry<NodeTemplate>: Search and filter operations
 * - ExportableRegistry<NodeTemplate>: Export/import operations
 */

import type { NodeTemplate, NodeTemplateSummary, StaticNode } from "@wf-agent/types";
import { StaticNodeType } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { validateNodeByType } from "../../workflow/validation/node-validation/index.js";
import { getErrorMessage, now } from "@wf-agent/common-utils";
import type { NodeTemplateStorageAdapter } from "@wf-agent/storage";
import { persistNodeTemplate, removeNodeTemplate } from "./utils/storage/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { createRegistry } from "./utils/index.js";
import type {
  Registry,
  MutableRegistry,
  SearchableRegistry,
  ExportableRegistry,
  BatchOperations,
} from "./types.js";
import {
  RegistryNotFoundError,
  RegistryAlreadyExistsError,
  RegistryValidationError,
} from "./types.js";
import { validateNodeTemplate } from "./utils/index.js";

const logger = createContextualLogger({ component: "NodeTemplateRegistry" });

/**
 * Node Template Registry Class
 *
 * Implements:
 * - Registry<NodeTemplate>: Read operations (get, has, list, keys, size, clear)
 * - MutableRegistry<NodeTemplate>: Write operations (set, delete)
 * - BatchOperations<NodeTemplate>: Batch register/unregister
 * - SearchableRegistry<NodeTemplate>: Search and filter
 * - ExportableRegistry<NodeTemplate>: Export/import
 */
class NodeTemplateRegistry
  implements
    Registry<NodeTemplate>,
    MutableRegistry<NodeTemplate>,
    BatchOperations<NodeTemplate>,
    SearchableRegistry<NodeTemplate>,
    ExportableRegistry<NodeTemplate>
{
  private items = createRegistry<NodeTemplate>();

  constructor(private readonly storageAdapter: NodeTemplateStorageAdapter | null = null) {}

  // ============================================================
  // Registry Interface Implementation (Read Operations)
  // ============================================================

  /** Get node template by name, returns undefined if not found */
  get(name: string): NodeTemplate | undefined {
    return this.items.get(name);
  }

  /** Check if node template exists */
  has(name: string): boolean {
    return this.items.has(name);
  }

  /** List all node templates */
  list(): NodeTemplate[] {
    return this.items.list();
  }

  /** Get all node template names */
  keys(): string[] {
    return this.items.keys();
  }

  /** Get the number of node templates */
  get size(): number {
    return this.items.size;
  }

  /** Clear all node templates */
  async clear(): Promise<void> {
    this.items.clear();
    if (this.storageAdapter) {
      await this.storageAdapter.clear();
    }
  }

  // ============================================================
  // MutableRegistry Interface Implementation (Write Operations)
  // ============================================================

  /** Set a node template by name */
  set(name: string, value: NodeTemplate): void {
    this.items.set(name, value);
  }

  /** Delete a node template by name, returns true if deleted */
  delete(name: string): boolean {
    return this.items.delete(name);
  }

  // ============================================================
  // Core CRUD Operations (Standardized Naming)
  // ============================================================

  /**
   * Register node template (memory-only, no persistence).
   * Used for predefined content registration during bootstrap.
   *
   * @param template - The node template to register
   * @throws RegistryValidationError If the template is invalid
   * @throws RegistryAlreadyExistsError If the name already exists
   */
  register(template: NodeTemplate): void {
    this.validateTemplate(template);

    if (this.items.has(template.name)) {
      throw new RegistryAlreadyExistsError(template.name, "Node template");
    }

    this.items.set(template.name, template);
    logger.info("Node template registered (memory-only)", { name: template.name });
  }

  /**
   * Register node template with storage persistence (write-through).
   *
   * @param template - The node template to register
   * @throws RegistryValidationError If the template is invalid
   * @throws RegistryAlreadyExistsError If the name already exists
   */
  async registerNodeTemplate(template: NodeTemplate): Promise<void> {
    this.validateTemplate(template);

    if (this.items.has(template.name)) {
      throw new RegistryAlreadyExistsError(template.name, "Node template");
    }

    // Persist to storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await persistNodeTemplate(template, this.storageAdapter);
    }

    this.items.set(template.name, template);
    logger.info("Node template registered", { name: template.name });
  }

  /**
   * Update node template (memory-only).
   *
   * @param name - Name of the node template
   * @param updates - Updates to apply
   * @throws RegistryNotFoundError If the template does not exist
   * @throws RegistryValidationError If the updated configuration is invalid
   */
  update(name: string, updates: Partial<NodeTemplate>): void {
    const template = this.items.get(name);
    if (!template) {
      throw new RegistryNotFoundError(name, "Node template");
    }

    const updatedTemplate: NodeTemplate = {
      ...template,
      ...updates,
      name: template.name, // Name cannot be changed
      updatedAt: now(),
    };

    this.validateTemplate(updatedTemplate);
    this.items.set(name, updatedTemplate);
    logger.info("Node template updated", { name });
  }

  /**
   * Update node template with storage persistence (write-through).
   *
   * @param name - Name of the node template
   * @param updates - Updates to apply
   * @throws RegistryNotFoundError If the template does not exist
   * @throws RegistryValidationError If the updated configuration is invalid
   */
  async updateNodeTemplate(name: string, updates: Partial<NodeTemplate>): Promise<void> {
    const template = this.items.get(name);
    if (!template) {
      throw new RegistryNotFoundError(name, "Node template");
    }

    const updatedTemplate: NodeTemplate = {
      ...template,
      ...updates,
      name: template.name, // Name cannot be changed
      updatedAt: now(),
    };

    this.validateTemplate(updatedTemplate);

    // Persist to storage first (write-through)
    if (this.storageAdapter) {
      await persistNodeTemplate(updatedTemplate, this.storageAdapter);
    }

    this.items.set(name, updatedTemplate);
    logger.info("Node template updated", { name });
  }

  /**
   * Unregister node template (memory-only).
   *
   * @param name - Name of the node template
   * @throws RegistryNotFoundError If the template does not exist
   */
  unregister(name: string): void {
    if (!this.items.has(name)) {
      throw new RegistryNotFoundError(name, "Node template");
    }
    this.items.delete(name);
    logger.info("Node template unregistered", { name });
  }

  /**
   * Unregister node template with storage persistence (write-through).
   *
   * @param name - Name of the node template
   * @throws RegistryNotFoundError If the template does not exist
   */
  async unregisterNodeTemplate(name: string): Promise<void> {
    if (!this.items.has(name)) {
      throw new RegistryNotFoundError(name, "Node template");
    }

    // Remove from storage first (write-through)
    if (this.storageAdapter) {
      await removeNodeTemplate(name, this.storageAdapter);
    }

    this.items.delete(name);
    logger.info("Node template unregistered", { name });
  }

  // ============================================================
  // BatchOperations Interface Implementation
  // ============================================================

  /**
   * Batch register node templates (memory-only).
   * Throws on first invalid template.
   *
   * @param templates - Array of node templates
   * @throws RegistryValidationError If any template is invalid
   * @throws RegistryAlreadyExistsError If any name already exists
   */
  async registerBatch(templates: NodeTemplate[]): Promise<void> {
    for (const template of templates) {
      this.register(template);
    }
  }

  /**
   * Batch unregister node templates (memory-only).
   * Throws on first non-existent template.
   *
   * @param names - Array of node template names
   * @throws RegistryNotFoundError If any template does not exist
   */
  async unregisterBatch(names: string[]): Promise<void> {
    for (const name of names) {
      this.unregister(name);
    }
  }

  /**
   * Batch unregister node templates with storage persistence.
   *
   * @param names - Array of node template names
   */
  async unregisterNodeTemplateBatch(names: string[]): Promise<void> {
    for (const name of names) {
      try {
        await this.unregisterNodeTemplate(name);
      } catch (error) {
        logger.error("Failed to unregister node template", {
          name,
          error: getErrorMessage(error),
        });
      }
    }
  }

  // ============================================================
  // SearchableRegistry Interface Implementation
  // ============================================================

  /**
   * Search node templates by keyword.
   *
   * @param query - Search keyword
   * @returns Array of matching node templates
   */
  search(query: string): NodeTemplate[] {
    const lowerQuery = query.toLowerCase();
    return this.list().filter((template) => {
      return (
        template.name.toLowerCase().includes(lowerQuery) ||
        template.description?.toLowerCase().includes(lowerQuery) ||
        (template.metadata?.["tags"] as string[])?.some((tag: string) =>
          tag.toLowerCase().includes(lowerQuery),
        ) ||
        (template.metadata?.["category"] as string)?.toLowerCase().includes(lowerQuery)
      );
    });
  }

  /**
   * List node templates by category.
   *
   * @param category - Category filter
   * @returns Array of node templates
   */
  listByCategory(category: string): NodeTemplate[] {
    return this.list().filter((template) => template.metadata?.["category"] === category);
  }

  /**
   * List node templates by tags.
   *
   * @param tags - Array of tags
   * @returns Array of node templates
   */
  listByTags(tags: string[]): NodeTemplate[] {
    return this.list().filter((template) => {
      const templateTags = (template.metadata?.["tags"] as string[]) || [];
      return tags.every((tag) => templateTags.includes(tag));
    });
  }

  // ============================================================
  // Additional Query Methods
  // ============================================================

  /**
   * List all node template summaries.
   *
   * @returns Array of node template summaries
   */
  listSummaries(): NodeTemplateSummary[] {
    return this.list().map((template) => {
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
   * List node templates by type.
   *
   * @param type - Static node type
   * @returns Array of node templates
   */
  listByType(type: StaticNodeType): NodeTemplate[] {
    return this.list().filter((template) => template.type === type);
  }

  // ============================================================
  // ExportableRegistry Interface Implementation
  // ============================================================

  /**
   * Export node template as JSON string.
   *
   * @param name - Name of the node template
   * @returns JSON string
   * @throws RegistryNotFoundError If the template does not exist
   */
  export(name: string): string {
    const template = this.items.get(name);
    if (!template) {
      throw new RegistryNotFoundError(name, "Node template");
    }
    return JSON.stringify(template, null, 2);
  }

  /**
   * Import node template from JSON string.
   *
   * @param json - JSON string
   * @returns Node template name
   * @throws RegistryValidationError If the JSON is invalid or configuration is incorrect
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
      throw new RegistryValidationError(
        `Failed to import node template: ${getErrorMessage(error)}`,
        undefined,
      );
    }
  }

  // ============================================================
  // Storage Operations
  // ============================================================

  /**
   * Initialize node templates from storage.
   * Loads all persisted templates into memory cache.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    const { initializeNodeTemplatesFromStorage } = await import(
      "./utils/storage/index.js"
    );
    await initializeNodeTemplatesFromStorage(this.storageAdapter, this.items);
  }

  // ============================================================
  // Validation
  // ============================================================

  /**
   * Validate node template.
   *
   * @param template - The node template to validate
   * @throws RegistryValidationError If validation fails
   */
  private validateTemplate(template: NodeTemplate): void {
    validateNodeTemplate(template);

    const validNodeTypes = [
      "START",
      "END",
      "VARIABLE",
      "FORK",
      "JOIN",
      "SUBGRAPH",
      "SCRIPT",
      "LLM",
      "USER_INTERACTION",
      "ROUTE",
      "CONTEXT_PROCESSOR",
      "LOOP_START",
      "LOOP_END",
      "START_FROM_TRIGGER",
      "CONTINUE_FROM_TRIGGER",
    ];

    if (!validNodeTypes.includes(template.type)) {
      throw new RegistryValidationError(
        `Invalid node type: ${template.type}`,
        "type",
      );
    }

    const mockNode = {
      id: "validation",
      type: template.type,
      name: template.name,
      config: template.config,
      outgoingEdgeIds: [],
      incomingEdgeIds: [],
    } as StaticNode;

    const validationResult = validateNodeByType(mockNode);
    if (validationResult.isErr()) {
      const errors = validationResult.error;
      const errorMessage = errors.length > 0 ? errors[0]!.message : "Unknown validation error";
      throw new RegistryValidationError(
        `Invalid node configuration for template '${template.name}': ${errorMessage}`,
        "config",
      );
    }
  }
}

/**
 * Export the NodeTemplateRegistry class
 */
export { NodeTemplateRegistry };

