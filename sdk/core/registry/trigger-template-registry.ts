/**
 * Trigger Template Registry
 * Responsible for the registration, querying, and management of trigger templates
 *
 * This module only exports class definitions; instances are managed uniformly through the SingletonRegistry.
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

/**
 * Trigger Template Registry Class
 */
class TriggerTemplateRegistry {
  private templates: Map<string, TriggerTemplate> = new Map();

  /**
   * Register trigger template (only for new triggers)
   * @param template: Trigger template
   * @param options: Registration options
   * @throws ValidationError: If the trigger configuration is invalid or the name already exists
   */
  register(template: TriggerTemplate, options?: RegisterOptions): void {
    // Verify trigger configuration
    this.validateTemplate(template);

    // Check if the name already exists.
    if (this.templates.has(template.name)) {
      if (options?.skipIfExists) {
        // Idempotent operation: Skip existing items
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

    // Register Trigger Template
    this.templates.set(template.name, template);
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
  update(name: string, updates: Partial<TriggerTemplate>, options?: UpdateOptions): void {
    const template = this.templates.get(name);
    if (!template) {
      if (options?.createIfNotExists) {
        // Allow for automatic creation.
        const newTemplate = { ...updates, name } as TriggerTemplate;
        this.register(newTemplate);
        return;
      }
      throw new TriggerTemplateNotFoundError(
        `Trigger template '${name}' not found. Use register() to create or upsert() to create or update.`,
        name,
      );
    }

    // Create the updated template.
    const updatedTemplate: TriggerTemplate = {
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
   * Register or update a trigger template (update if it exists, create if it doesn't).
   * @param template Trigger template
   */
  upsert(template: TriggerTemplate): void {
    if (this.templates.has(template.name)) {
      this.update(template.name, template);
    } else {
      this.register(template);
    }
  }

  /**
   * Get trigger template
   * @param name Name of the trigger template
   * @returns The trigger template; returns undefined if it does not exist
   */
  get(name: string): TriggerTemplate | undefined {
    return this.templates.get(name);
  }

  /**
   * Check if the trigger template exists
   * @param name Name of the trigger template
   * @returns Whether it exists
   */
  has(name: string): boolean {
    return this.templates.has(name);
  }

  /**
   * Delete trigger template
   * @param name Name of the trigger template
   * @param options Options for deletion
   * @throws NotFoundError If the trigger template does not exist
   */
  unregister(name: string, options?: UnregisterOptions): void {
    if (!this.templates.has(name)) {
      throw new TriggerTemplateNotFoundError(`Trigger template '${name}' not found`, name);
    }

    // Note: Reference checking is expected to be done at the API layer
    // This method accepts options for interface consistency with WorkflowRegistry
    // The force and checkReferences options should be handled by the caller

    this.templates.delete(name);
  }

  /**
   * Batch delete trigger templates
   * @param names Array of trigger template names
   * @param options Options for batch deletion
   */
  unregisterBatch(names: string[], options?: BatchUnregisterOptions): void {
    for (const name of names) {
      try {
        this.unregister(name, options);
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
    return Array.from(this.templates.values());
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
    this.templates.clear();
  }

  /**
   * Get the number of trigger templates
   * @returns The number of trigger templates
   */
  size(): number {
    return this.templates.size;
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
   * Verify trigger template
   * @param template: Trigger template
   * @throws ValidationError: If the trigger configuration is invalid
   */
  private validateTemplate(template: TriggerTemplate): void {
    // Verify required fields
    if (!template.name || typeof template.name !== "string") {
      throw new ConfigurationValidationError(
        "Trigger template name is required and must be a string",
        {
          configType: "trigger",
          configPath: "template.name",
        },
      );
    }

    if (!template.condition) {
      throw new ConfigurationValidationError("Trigger template condition is required", {
        configType: "trigger",
        configPath: "template.condition",
      });
    }

    if (!template.action) {
      throw new ConfigurationValidationError("Trigger template action is required", {
        configType: "trigger",
        configPath: "template.action",
      });
    }

    // Verify the trigger conditions.
    if (!template.condition.eventType) {
      throw new ConfigurationValidationError("Trigger template condition eventType is required", {
        configType: "trigger",
        configPath: "template.condition.eventType",
      });
    }

    // Verify whether the event type is valid.
    const validEventTypes = [
      "THREAD_STARTED",
      "THREAD_COMPLETED",
      "THREAD_FAILED",
      "THREAD_PAUSED",
      "THREAD_RESUMED",
      "THREAD_CANCELLED",
      "THREAD_STATE_CHANGED",
      "THREAD_FORK_STARTED",
      "THREAD_FORK_COMPLETED",
      "THREAD_JOIN_STARTED",
      "THREAD_JOIN_CONDITION_MET",
      "THREAD_COPY_STARTED",
      "THREAD_COPY_COMPLETED",
      "NODE_STARTED",
      "NODE_COMPLETED",
      "NODE_FAILED",
      "NODE_CUSTOM_EVENT",
      "TOKEN_LIMIT_EXCEEDED",
      "TOKEN_USAGE_WARNING",
      "CONTEXT_COMPRESSION_REQUESTED",
      "CONTEXT_COMPRESSION_COMPLETED",
      "MESSAGE_ADDED",
      "TOOL_CALL_STARTED",
      "TOOL_CALL_COMPLETED",
      "TOOL_CALL_FAILED",
      "TOOL_ADDED",
      "CONVERSATION_STATE_CHANGED",
      "ERROR",
      "CHECKPOINT_CREATED",
      "CHECKPOINT_RESTORED",
      "CHECKPOINT_DELETED",
      "CHECKPOINT_FAILED",
      "SUBGRAPH_STARTED",
      "SUBGRAPH_COMPLETED",
      "TRIGGERED_SUBGRAPH_STARTED",
      "TRIGGERED_SUBGRAPH_COMPLETED",
      "TRIGGERED_SUBGRAPH_FAILED",
      "VARIABLE_CHANGED",
      "USER_INTERACTION_REQUESTED",
      "USER_INTERACTION_RESPONDED",
      "USER_INTERACTION_PROCESSED",
      "USER_INTERACTION_FAILED",
      "HUMAN_RELAY_REQUESTED",
      "HUMAN_RELAY_RESPONDED",
      "HUMAN_RELAY_PROCESSED",
      "HUMAN_RELAY_FAILED",
      "LLM_STREAM_ABORTED",
      "LLM_STREAM_ERROR",
      "DYNAMIC_THREAD_SUBMITTED",
      "DYNAMIC_THREAD_COMPLETED",
      "DYNAMIC_THREAD_FAILED",
      "DYNAMIC_THREAD_CANCELLED",
    ];
    if (!validEventTypes.includes(template.condition.eventType)) {
      throw new ConfigurationValidationError(
        `Invalid event type: ${template.condition.eventType}`,
        {
          configType: "trigger",
          configPath: "template.condition.eventType",
        },
      );
    }

    // Verify the trigger action.
    if (!template.action.type) {
      throw new ConfigurationValidationError("Trigger template action type is required", {
        configType: "trigger",
        configPath: "template.action.type",
      });
    }

    // Verify whether the action type is valid.
    const validActionTypes = [
      "stop_thread",
      "pause_thread",
      "resume_thread",
      "skip_node",
      "set_variable",
      "send_notification",
      "custom",
      "execute_triggered_subgraph",
      "execute_script",
      "apply_message_operation",
    ];
    if (!validActionTypes.includes(template.action.type)) {
      throw new ConfigurationValidationError(`Invalid action type: ${template.action.type}`, {
        configType: "trigger",
        configPath: "template.action.type",
      });
    }
  }

  /**
   * Export the trigger template as a JSON string
   * @param name The name of the trigger template
   * @returns A JSON string
   * @throws NotFoundError If the trigger template does not exist
   */
  export(name: string): string {
    const template = this.templates.get(name);
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
}

/**
 * Export the TriggerTemplateRegistry class
 */
export { TriggerTemplateRegistry };
