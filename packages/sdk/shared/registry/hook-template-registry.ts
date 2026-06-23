/**
 * Hook Template Registry
 *
 * Responsible for the registration, querying, and management of hook templates.
 * Implements standardized registry interfaces for consistency.
 *
 * This module only exports class definitions; instances are managed by the DI container as singletons.
 *
 * Interface Implementation:
 * - Registry<HookTemplate>: Read operations
 * - MutableRegistry<HookTemplate>: Write operations
 * - BatchOperations<HookTemplate>: Batch register/unregister
 * - SearchableRegistry<HookTemplate>: Search and filter operations
 * - ExportableRegistry<HookTemplate>: Export/import operations
 */

import type { HookTemplate, HookTemplateSummary } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { getErrorMessage, now } from "@wf-agent/common-utils";
import type { HookTemplateStorageAdapter } from "@wf-agent/storage";
import { persistHookTemplate, removeHookTemplate } from "./utils/storage/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { createRegistry } from "./utils/index.js";
import type {
  Registry,
  MutableRegistry,
  BatchOperations,
  SearchableRegistry,
  ExportableRegistry,
} from "./types.js";
import {
  RegistryNotFoundError,
  RegistryAlreadyExistsError,
  RegistryValidationError,
} from "./types.js";
import { validateHookTemplate } from "./utils/index.js";

/**
 * Hook Template Registry Class
 *
 * Implements:
 * - Registry<HookTemplate>: Read operations (get, has, list, keys, size, clear)
 * - MutableRegistry<HookTemplate>: Write operations (set, delete)
 * - BatchOperations<HookTemplate>: Batch register/unregister
 * - SearchableRegistry<HookTemplate>: Search and filter
 * - ExportableRegistry<HookTemplate>: Export/import
 */
class HookTemplateRegistry
  implements
    Registry<HookTemplate>,
    MutableRegistry<HookTemplate>,
    BatchOperations<HookTemplate>,
    SearchableRegistry<HookTemplate>,
    ExportableRegistry<HookTemplate>
{
  private items = createRegistry<HookTemplate>();

  constructor(private readonly storageAdapter: HookTemplateStorageAdapter | null = null) {}

  // ============================================================
  // Registry Interface Implementation (Read Operations)
  // ============================================================

  /** Get hook template by name, returns undefined if not found */
  get(name: string): HookTemplate | undefined {
    return this.items.get(name);
  }

  /** Check if hook template exists */
  has(name: string): boolean {
    return this.items.has(name);
  }

  /** List all hook templates */
  list(): HookTemplate[] {
    return this.items.list();
  }

  /** Get all hook template names */
  keys(): string[] {
    return this.items.keys();
  }

  /** Get the number of hook templates */
  get size(): number {
    return this.items.size;
  }

  /** Clear all hook templates */
  clear(): void {
    this.items.clear();
  }

  // ============================================================
  // MutableRegistry Interface Implementation (Write Operations)
  // ============================================================

  /** Set a hook template by name */
  set(name: string, value: HookTemplate): void {
    this.items.set(name, value);
  }

  /** Delete a hook template by name, returns true if deleted */
  delete(name: string): boolean {
    return this.items.delete(name);
  }

  // ============================================================
  // Core CRUD Operations (Standardized Naming)
  // ============================================================

  /**
   * Register hook template (memory-only, no persistence).
   *
   * @param template - Hook template to register
   * @throws RegistryValidationError If the template is invalid
   * @throws RegistryAlreadyExistsError If the name already exists
   */
  register(template: HookTemplate): void {
    this.validateTemplate(template);

    if (this.items.has(template.name)) {
      throw new RegistryAlreadyExistsError(template.name, "Hook template");
    }

    this.items.set(template.name, template);
    logger.info("Hook template registered (memory-only)", { name: template.name });
  }

  /**
   * Register hook template with storage persistence (write-through).
   *
   * @param template - Hook template to register
   * @throws RegistryValidationError If the template is invalid
   * @throws RegistryAlreadyExistsError If the name already exists
   */
  async registerHookTemplate(template: HookTemplate): Promise<void> {
    this.validateTemplate(template);

    if (this.items.has(template.name)) {
      throw new RegistryAlreadyExistsError(template.name, "Hook template");
    }

    // Persist to storage first (write-through)
    if (this.storageAdapter) {
      await persistHookTemplate(template, this.storageAdapter);
    }

    this.items.set(template.name, template);
    logger.info("Hook template registered", { name: template.name });
  }

  /**
   * Update hook template (memory-only).
   *
   * @param name - Name of the hook template
   * @param updates - Updates to apply
   * @throws RegistryNotFoundError If the template does not exist
   * @throws RegistryValidationError If the updated configuration is invalid
   */
  update(name: string, updates: Partial<HookTemplate>): void {
    const template = this.items.get(name);
    if (!template) {
      throw new RegistryNotFoundError(name, "Hook template");
    }

    const updatedTemplate: HookTemplate = {
      ...template,
      ...updates,
      name: template.name, // Name cannot be changed
      updatedAt: now(),
    };

    this.validateTemplate(updatedTemplate);
    this.items.set(name, updatedTemplate);
    logger.info("Hook template updated", { name });
  }

  /**
   * Update hook template with storage persistence (write-through).
   *
   * @param name - Name of the hook template
   * @param updates - Updates to apply
   * @throws RegistryNotFoundError If the template does not exist
   * @throws RegistryValidationError If the updated configuration is invalid
   */
  async updateHookTemplate(name: string, updates: Partial<HookTemplate>): Promise<void> {
    const template = this.items.get(name);
    if (!template) {
      throw new RegistryNotFoundError(name, "Hook template");
    }

    const updatedTemplate: HookTemplate = {
      ...template,
      ...updates,
      name: template.name, // Name cannot be changed
      updatedAt: now(),
    };

    this.validateTemplate(updatedTemplate);

    // Persist to storage first (write-through)
    if (this.storageAdapter) {
      await persistHookTemplate(updatedTemplate, this.storageAdapter);
    }

    this.items.set(name, updatedTemplate);
    logger.info("Hook template updated", { name });
  }

  /**
   * Unregister hook template (memory-only).
   *
   * @param name - Name of the hook template
   * @throws RegistryNotFoundError If the template does not exist
   */
  unregister(name: string): void {
    if (!this.items.has(name)) {
      throw new RegistryNotFoundError(name, "Hook template");
    }
    this.items.delete(name);
    logger.info("Hook template unregistered", { name });
  }

  /**
   * Unregister hook template with storage persistence (write-through).
   *
   * @param name - Name of the hook template
   * @throws RegistryNotFoundError If the template does not exist
   */
  async unregisterHookTemplate(name: string): Promise<void> {
    if (!this.items.has(name)) {
      throw new RegistryNotFoundError(name, "Hook template");
    }

    // Remove from storage first (write-through)
    if (this.storageAdapter) {
      await removeHookTemplate(name, this.storageAdapter);
    }

    this.items.delete(name);
    logger.info("Hook template unregistered", { name });
  }

  // ============================================================
  // BatchOperations Interface Implementation
  // ============================================================

  /**
   * Batch register hook templates (memory-only).
   *
   * @param templates - Array of hook templates
   */
  async registerBatch(templates: HookTemplate[]): Promise<void> {
    for (const template of templates) {
      this.register(template);
    }
  }

  /**
   * Batch unregister hook templates (memory-only).
   *
   * @param names - Array of hook template names
   */
  async unregisterBatch(names: string[]): Promise<void> {
    for (const name of names) {
      this.unregister(name);
    }
  }

  // ============================================================
  // SearchableRegistry Interface Implementation
  // ============================================================

  /**
   * Search hook templates by keyword.
   *
   * @param query - Search keyword
   * @returns Array of matching hook templates
   */
  search(query: string): HookTemplate[] {
    const lowerQuery = query.toLowerCase();
    return this.list().filter((template) => {
      return (
        template.name.toLowerCase().includes(lowerQuery) ||
        template.description?.toLowerCase().includes(lowerQuery) ||
        template.hook.hookType.toLowerCase().includes(lowerQuery) ||
        template.hook.eventName.toLowerCase().includes(lowerQuery) ||
        (template.metadata?.["tags"] as string[])?.some((tag: string) =>
          tag.toLowerCase().includes(lowerQuery),
        ) ||
        (template.metadata?.["category"] as string)?.toLowerCase().includes(lowerQuery)
      );
    });
  }

  /**
   * List hook templates by category.
   *
   * @param category - Category filter
   * @returns Array of hook templates
   */
  listByCategory(category: string): HookTemplate[] {
    return this.list().filter((template) => template.metadata?.["category"] === category);
  }

  /**
   * List hook templates by tags.
   *
   * @param tags - Array of tags
   * @returns Array of hook templates
   */
  listByTags(tags: string[]): HookTemplate[] {
    return this.list().filter((template) => {
      const templateTags = (template.metadata?.["tags"] as string[]) || [];
      return tags.every((tag) => templateTags.includes(tag));
    });
  }

  // ============================================================
  // Additional Query Methods
  // ============================================================

  /**
   * List all hook template summaries.
   *
   * @returns Array of hook template summaries
   */
  listSummaries(): HookTemplateSummary[] {
    return this.list().map((template) => {
      const summary: HookTemplateSummary = {
        name: template.name,
        hookType: template.hook.hookType,
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
   * List hook templates by hook type.
   *
   * @param hookType - Hook type filter
   * @returns Array of hook templates
   */
  listByHookType(hookType: string): HookTemplate[] {
    return this.list().filter((template) => template.hook.hookType === hookType);
  }

  // ============================================================
  // ExportableRegistry Interface Implementation
  // ============================================================

  /**
   * Export hook template as JSON string.
   *
   * @param name - Name of the hook template
   * @returns JSON string
   * @throws RegistryNotFoundError If the template does not exist
   */
  export(name: string): string {
    const template = this.items.get(name);
    if (!template) {
      throw new RegistryNotFoundError(name, "Hook template");
    }
    return JSON.stringify(template, null, 2);
  }

  /**
   * Import hook template from JSON string.
   *
   * @param json - JSON string
   * @returns Hook template name
   * @throws RegistryValidationError If the JSON is invalid or configuration is incorrect
   */
  import(json: string): string {
    try {
      const template = JSON.parse(json) as HookTemplate;
      this.register(template);
      return template.name;
    } catch (error) {
      if (error instanceof ValidationError) {
        throw error;
      }
      throw new RegistryValidationError(
        `Failed to import hook template: ${getErrorMessage(error)}`,
        undefined,
      );
    }
  }

  // ============================================================
  // Storage Operations
  // ============================================================

  /**
   * Initialize hook templates from storage.
   * Loads all persisted templates into memory cache.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    const { initializeHookTemplatesFromStorage } = await import(
      "./utils/storage/index.js"
    );
    await initializeHookTemplatesFromStorage(this.storageAdapter, this.items);
  }

  // ============================================================
  // Validation
  // ============================================================

  /**
   * Validate hook template.
   *
   * @param template - The hook template to validate
   * @throws RegistryValidationError If validation fails
   */
  private validateTemplate(template: HookTemplate): void {
    validateHookTemplate(template);
  }
}

const logger = createContextualLogger({ component: "HookTemplateRegistry" });

/**
 * Export the HookTemplateRegistry class
 */
export { HookTemplateRegistry };

