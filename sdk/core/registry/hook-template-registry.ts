/**
 * HookTemplate Registry
 * Responsible for the registration, querying, and management of hook templates.
 *
 * This module only exports class definitions; instances are managed by the DI container as singletons.
 */

import type { HookTemplate, HookTemplateSummary } from "@wf-agent/types";
import {
  ValidationError,
  ConfigurationValidationError,
  HookTemplateNotFoundError,
} from "@wf-agent/types";
import { getErrorMessage, now } from "@wf-agent/common-utils";
import type { HookTemplateStorageAdapter } from "@wf-agent/storage";
import { persistHookTemplate, removeHookTemplate } from "./utils/hook-template-storage-utils.js";

/**
 * HookTemplate Registry Class
 */
class HookTemplateRegistry {
  private templates: Map<string, HookTemplate> = new Map();

  constructor(private readonly storageAdapter: HookTemplateStorageAdapter | null = null) {}

  /**
   * Register a hook template.
   * @param template - The hook template
   * @throws ValidationError If the hook configuration is invalid or the name already exists
   */
  register(template: HookTemplate): void {
    this.validateTemplate(template);

    if (this.templates.has(template.name)) {
      throw new ConfigurationValidationError(
        `Hook template with name '${template.name}' already exists`,
        {
          configType: "hook_template",
          configPath: "template.name",
        },
      );
    }

    this.templates.set(template.name, template);
  }

  /**
   * Batch register hook templates.
   * @param templates - Array of hook templates
   */
  registerBatch(templates: HookTemplate[]): void {
    for (const template of templates) {
      this.register(template);
    }
  }

  /**
   * Register Hook Template with storage persistence (write-through).
   * @param template - Hook template to register
   * @throws ConfigurationValidationError If the template is invalid or already exists
   */
  async registerHookTemplate(template: HookTemplate): Promise<void> {
    this.validateTemplate(template);

    if (this.templates.has(template.name)) {
      throw new ConfigurationValidationError(
        `Hook template with name '${template.name}' already exists`,
        {
          configType: "hook_template",
          configPath: "template.name",
        },
      );
    }

    // Persist to storage first (write-through)
    if (this.storageAdapter) {
      await persistHookTemplate(template, this.storageAdapter);
    }

    this.templates.set(template.name, template);
  }

  /**
   * Update Hook Template with storage persistence (write-through).
   * @param name - Name of the hook template
   * @param updates - Updates to apply
   * @throws HookTemplateNotFoundError If the hook template does not exist
   * @throws ValidationError If the updated configuration is invalid
   */
  async updateHookTemplate(name: string, updates: Partial<HookTemplate>): Promise<void> {
    const template = this.templates.get(name);
    if (!template) {
      throw new HookTemplateNotFoundError(`Hook template '${name}' not found`, name);
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

    this.templates.set(name, updatedTemplate);
  }

  /**
   * Unregister Hook Template with storage persistence (write-through).
   * @param name - Name of the hook template
   * @throws HookTemplateNotFoundError If the hook template does not exist
   */
  async unregisterHookTemplate(name: string): Promise<void> {
    if (!this.templates.has(name)) {
      throw new HookTemplateNotFoundError(`Hook template '${name}' not found`, name);
    }

    // Remove from storage first (write-through)
    if (this.storageAdapter) {
      await removeHookTemplate(name, this.storageAdapter);
    }

    this.templates.delete(name);
  }

  /**
   * Get a hook template by name.
   * @param name - Name of the hook template
   * @returns The hook template, or undefined if it does not exist
   */
  get(name: string): HookTemplate | undefined {
    return this.templates.get(name);
  }

  /**
   * Check if a hook template exists.
   * @param name - Name of the hook template
   * @returns Whether it exists
   */
  has(name: string): boolean {
    return this.templates.has(name);
  }

  /**
   * Update a hook template.
   * @param name - Name of the hook template
   * @param updates - Updates to apply
   * @throws HookTemplateNotFoundError If the hook template does not exist
   * @throws ValidationError If the updated configuration is invalid
   */
  update(name: string, updates: Partial<HookTemplate>): void {
    const template = this.templates.get(name);
    if (!template) {
      throw new HookTemplateNotFoundError(`Hook template '${name}' not found`, name);
    }

    const updatedTemplate: HookTemplate = {
      ...template,
      ...updates,
      name: template.name, // Name cannot be changed
      updatedAt: now(),
    };

    this.validateTemplate(updatedTemplate);
    this.templates.set(name, updatedTemplate);
  }

  /**
   * Delete a hook template.
   * @param name - Name of the hook template
   * @throws HookTemplateNotFoundError If the hook template does not exist
   */
  unregister(name: string): void {
    if (!this.templates.has(name)) {
      throw new HookTemplateNotFoundError(`Hook template '${name}' not found`, name);
    }
    this.templates.delete(name);
  }

  /**
   * Batch delete hook templates.
   * @param names - Array of hook template names
   */
  unregisterBatch(names: string[]): void {
    for (const name of names) {
      this.unregister(name);
    }
  }

  /**
   * List all hook templates.
   * @returns Array of hook templates
   */
  list(): HookTemplate[] {
    return Array.from(this.templates.values());
  }

  /**
   * List all hook template summaries.
   * @returns Array of hook template summaries
   */
  listSummaries(): HookTemplateSummary[] {
    return this.list().map(template => {
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
   * @param hookType - Hook type filter
   * @returns Array of hook templates
   */
  listByHookType(hookType: string): HookTemplate[] {
    return this.list().filter(template => template.hook.hookType === hookType);
  }

  /**
   * List hook templates by category.
   * @param category - Category filter
   * @returns Array of hook templates
   */
  listByCategory(category: string): HookTemplate[] {
    return this.list().filter(template => template.metadata?.["category"] === category);
  }

  /**
   * List hook templates by tag.
   * @param tags - Array of tags
   * @returns Array of hook templates
   */
  listByTags(tags: string[]): HookTemplate[] {
    return this.list().filter(template => {
      const templateTags = (template.metadata?.["tags"] as string[]) || [];
      return tags.every(tag => templateTags.includes(tag));
    });
  }

  /**
   * Clear all hook templates.
   */
  clear(): void {
    this.templates.clear();
  }

  /**
   * Get the number of hook templates.
   * @returns The count
   */
  size(): number {
    return this.templates.size;
  }

  /**
   * Search for hook templates by keyword.
   * @param keyword - Search keyword
   * @returns Array of matching hook templates
   */
  search(keyword: string): HookTemplate[] {
    const lowerKeyword = keyword.toLowerCase();
    return this.list().filter(template => {
      return (
        template.name.toLowerCase().includes(lowerKeyword) ||
        template.description?.toLowerCase().includes(lowerKeyword) ||
        template.hook.hookType.toLowerCase().includes(lowerKeyword) ||
        template.hook.eventName.toLowerCase().includes(lowerKeyword) ||
        (template.metadata?.["tags"] as string[])?.some((tag: string) =>
          tag.toLowerCase().includes(lowerKeyword),
        ) ||
        (template.metadata?.["category"] as string)?.toLowerCase().includes(lowerKeyword)
      );
    });
  }

  /**
   * Validate a hook template.
   * @param template - The hook template to validate
   * @throws ValidationError If validation fails
   */
  private validateTemplate(template: HookTemplate): void {
    if (!template.name || typeof template.name !== "string") {
      throw new ConfigurationValidationError(
        "Hook template name is required and must be a string",
        {
          configType: "hook_template",
          configPath: "template.name",
        },
      );
    }

    if (!template.hook) {
      throw new ConfigurationValidationError("Hook template 'hook' configuration is required", {
        configType: "hook_template",
        configPath: "template.hook",
      });
    }

    if (!template.hook.hookType) {
      throw new ConfigurationValidationError("Hook template hook.hookType is required", {
        configType: "hook_template",
        configPath: "template.hook.hookType",
      });
    }

    if (!template.hook.eventName) {
      throw new ConfigurationValidationError("Hook template hook.eventName is required", {
        configType: "hook_template",
        configPath: "template.hook.eventName",
      });
    }
  }

  /**
   * Export a hook template as a JSON string.
   * @param name - Name of the hook template
   * @returns JSON string
   * @throws HookTemplateNotFoundError If the hook template does not exist
   */
  export(name: string): string {
    const template = this.templates.get(name);
    if (!template) {
      throw new HookTemplateNotFoundError(`Hook template '${name}' not found`, name);
    }
    return JSON.stringify(template, null, 2);
  }

  /**
   * Import a hook template from a JSON string.
   * @param json - JSON string
   * @returns Hook template name
   * @throws ValidationError If the JSON is invalid or the hook configuration is invalid
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
      throw new ConfigurationValidationError(
        `Failed to import hook template: ${getErrorMessage(error)}`,
        {
          configType: "hook_template",
          configPath: "json",
        },
      );
    }
  }
}

/**
 * Export the HookTemplateRegistry class
 */
export { HookTemplateRegistry };
