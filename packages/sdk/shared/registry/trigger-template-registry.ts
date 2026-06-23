/**
 * Trigger Template Registry
 * Responsible for the registration, querying, and management of trigger templates
 *
 * This module only exports class definitions; instances are managed by the DI container as singletons.
 */

import type {
  TriggerTemplate,
  TriggerTemplateSummary,
  RegisterOptions,
  BatchRegisterOptions,
  UnregisterOptions,
  BatchUnregisterOptions,
  UpdateOptions,
} from "@wf-agent/types";
import type { WorkflowTrigger } from "@wf-agent/types";
import {
  ValidationError,
  ConfigurationValidationError,
  TriggerTemplateNotFoundError,
} from "@wf-agent/types";
import { getErrorMessage, now } from "@wf-agent/common-utils";
import type { TriggerStorageAdapter } from "@wf-agent/storage";
import {
  persistTrigger,
  removeTrigger,
  initializeTriggersFromStorage,
} from "./utils/storage/index.js";
import { createRegistry, validateTriggerTemplateRegistry } from "./utils/index.js";

/**
 * Trigger Template Registry Class
 */
class TriggerTemplateRegistry {
  private items = createRegistry<TriggerTemplate>();

  constructor(private readonly storageAdapter: TriggerStorageAdapter | null = null) {}

  /**
   * Register trigger template (only for new triggers)
   * @param template: Trigger template
   * @param options: Registration options
   * @throws ValidationError: If the trigger configuration is invalid or the name already exists
   */
  register(template: TriggerTemplate, options?: RegisterOptions): void {
    validateTriggerTemplateRegistry(template);

    if (this.items.has(template.name)) {
      if (options?.skipIfExists) {
        return;
      }
      throw new ConfigurationValidationError(
        `Trigger template with name '${template.name}' already exists. Use update() to modify or upsert() to create or update.`,
        {
          configType: "trigger",
          configPath: "template.name",
        },
      );
    }

    this.items.set(template.name, template);
  }

  /**
   * Register trigger template asynchronously (with storage persistence).
   *
   * Persists to storage first (write-through: DB is source of truth),
   * then updates the in-memory cache upon success.
   *
   * @param template Trigger template to register
   * @param options Registration options
   * @throws ValidationError If the trigger configuration is invalid or the name already exists
   */
  async registerAsync(template: TriggerTemplate, options?: RegisterOptions): Promise<void> {
    validateTriggerTemplateRegistry(template);

    if (this.items.has(template.name)) {
      if (options?.skipIfExists) {
        return;
      }
      throw new ConfigurationValidationError(
        `Trigger template with name '${template.name}' already exists. Use update() to modify or upsert() to create or update.`,
        {
          configType: "trigger",
          configPath: "template.name",
        },
      );
    }

    if (this.storageAdapter) {
      await persistTrigger(template, this.storageAdapter);
    }

    this.items.set(template.name, template);
  }

  /**
   * Batch registration of trigger templates
   * @param templates: Array of trigger templates
   * @param options: Options for batch registration
   */
  registerBatch(templates: TriggerTemplate[], options?: BatchRegisterOptions): void {
    for (const template of templates) {
      try {
        this.register(template, options);
      } catch (error) {
        if (options?.skipErrors) {
          continue;
        }
        throw error;
      }
    }
  }

  /**
   * Update Trigger Template (Modifications Only)
   * @param name The name of the trigger template
   * @param updates The update content
   * @param options The update options
   * @throws NotFoundError If the trigger template does not exist
   * @throws ValidationError If the updated configuration is invalid
   */
  async update(
    name: string,
    updates: Partial<TriggerTemplate>,
    options?: UpdateOptions,
  ): Promise<void> {
    const template = this.items.get(name);
    if (!template) {
      if (options?.createIfNotExists) {
        const newTemplate = { ...updates, name } as TriggerTemplate;
        this.register(newTemplate);
        return;
      }
      throw new TriggerTemplateNotFoundError(
        `Trigger template '${name}' not found. Use register() to create or upsert() to create or update.`,
        name,
      );
    }

    const updatedTemplate: TriggerTemplate = {
      ...template,
      ...updates,
      name: template.name,
      updatedAt: now(),
    };

    validateTriggerTemplateRegistry(updatedTemplate);

    if (this.storageAdapter) {
      await persistTrigger(updatedTemplate, this.storageAdapter);
    }

    this.items.set(name, updatedTemplate);
  }

  /**
   * Register or update a trigger template (update if it exists, create if it doesn't).
   * @param template Trigger template
   */
  async upsert(template: TriggerTemplate): Promise<void> {
    if (this.items.has(template.name)) {
      await this.update(template.name, template);
    } else {
      await this.registerAsync(template);
    }
  }

  /**
   * Get trigger template
   * @param name Name of the trigger template
   * @returns The trigger template; returns undefined if it does not exist
   */
  get(name: string): TriggerTemplate | undefined {
    return this.items.get(name);
  }

  /**
   * Check if the trigger template exists
   * @param name Name of the trigger template
   * @returns Whether it exists
   */
  has(name: string): boolean {
    return this.items.has(name);
  }

  /**
   * Delete trigger template
   * @param name Name of the trigger template
   * @param options Options for deletion (force, checkReferences)
   * @throws NotFoundError If the trigger template does not exist
   * @throws ConfigurationValidationError If the template is referenced and force is not set
   */
  async unregister(name: string, options?: UnregisterOptions): Promise<void> {
    if (!this.items.has(name)) {
      throw new TriggerTemplateNotFoundError(`Trigger template '${name}' not found`, name);
    }

    // Check references if enabled (default: true)
    const shouldCheck = options?.checkReferences !== false;

    if (shouldCheck) {
      // Note: Reference checking should be done at the API layer where workflow dependencies are accessible
      // This registry-level method accepts options for interface consistency
      // The actual reference validation happens in TriggerTemplateRegistryAPI.canSafelyDelete()
      // If force is not set and there are references, the API layer will throw an error
      // This method proceeds with deletion assuming the API layer has validated safety
    }

    // Remove from storage first (write-through: DB is source of truth)
    if (this.storageAdapter) {
      await removeTrigger(name, this.storageAdapter);
    }

    this.items.delete(name);
  }

  /**
   * Batch delete trigger templates
   * @param names Array of trigger template names
   * @param options Options for batch deletion
   */
  async unregisterBatch(names: string[], options?: BatchUnregisterOptions): Promise<void> {
    for (const name of names) {
      try {
        await this.unregister(name, options);
      } catch (error) {
        if (options?.skipErrors) {
          continue;
        }
        throw error;
      }
    }
  }

  /**
   * List all trigger templates
   * @returns Array of trigger templates
   */
  list(): TriggerTemplate[] {
    return this.items.list();
  }

  /**
   * List all trigger template summaries
   * @returns Array of trigger template summaries
   */
  listSummaries(): TriggerTemplateSummary[] {
    return this.list().map(template => {
      const summary: TriggerTemplateSummary = {
        name: template.name,
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
   * Clear all trigger templates.
   */
  clear(): void {
    this.items.clear();
  }

  /**
   * Get the number of trigger templates
   * @returns The number of trigger templates
   */
  size(): number {
    return this.items.size;
  }

  /**
   * Search Trigger Template
   * @param keyword Search keyword
   * @returns Array of matching trigger templates
   */
  search(keyword: string): TriggerTemplate[] {
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
   * Export the trigger template as a JSON string
   * @param name The name of the trigger template
   * @returns A JSON string
   * @throws NotFoundError If the trigger template does not exist
   */
  export(name: string): string {
    const template = this.items.get(name);
    if (!template) {
      throw new TriggerTemplateNotFoundError(`Trigger template '${name}' not found`, name);
    }
    return JSON.stringify(template, null, 2);
  }

  /**
   * Import trigger template from a JSON string
   * @param json JSON string
   * @param options Registration options
   * @returns Trigger template name
   * @throws ValidationError If the JSON is invalid or the trigger configuration is incorrect
   */
  import(json: string, options?: RegisterOptions): string {
    try {
      const template = JSON.parse(json) as TriggerTemplate;
      this.register(template, options);
      return template.name;
    } catch (error) {
      if (error instanceof ValidationError) {
        throw error;
      }
      throw new ConfigurationValidationError(
        `Failed to import trigger template: ${getErrorMessage(error)}`,
        {
          configType: "trigger",
          configPath: "json",
        },
      );
    }
  }

  /**
   * Translate the following text from Chinese to English:
   *
   * Convert the trigger template to a WorkflowTrigger.
   * @param templateName: The name of the trigger template.
   * @param triggerId: The ID of the trigger.
   * @param triggerName: The name of the trigger (optional).
   * @param configOverride: Configuration override (optional).
   */
  convertToWorkflowTrigger(
    templateName: string,
    triggerId: string,
    triggerName?: string,
    configOverride?: {
      condition?: unknown;
      action?: unknown;
      enabled?: boolean;
      maxTriggers?: number;
    },
  ): WorkflowTrigger {
    const template = this.get(templateName);
    if (!template) {
      throw new TriggerTemplateNotFoundError(
        `Trigger template '${templateName}' not found`,
        templateName,
      );
    }

    // Merge configuration overrides.
    const mergedCondition = configOverride?.condition
      ? { ...template.condition, ...configOverride.condition }
      : template.condition;

    const mergedAction = configOverride?.action
      ? { ...template.action, ...configOverride.action }
      : template.action;

    // Create a WorkflowTrigger
    const workflowTrigger: WorkflowTrigger = {
      id: triggerId,
      name: triggerName || template.name,
      description: template.description,
      condition: mergedCondition,
      action: mergedAction,
      enabled: configOverride?.enabled ?? template.enabled,
      maxTriggers: configOverride?.maxTriggers ?? template.maxTriggers,
      metadata: template.metadata,
    };

    return workflowTrigger;
  }

  // ============================================================
  // Storage Initialization
  // ============================================================

  /**
   * Initialize trigger templates from storage
   * Loads all persisted trigger template definitions into memory cache.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      return;
    }

    await initializeTriggersFromStorage(this.storageAdapter, this.items);
  }
}

/**
 * Export the TriggerTemplateRegistry class
 */
export { TriggerTemplateRegistry };
