/**
 * Trigger Template Registry API
 * Provides a user-friendly API interface for managing trigger templates.
 * Revised version: Inherits from GenericResourceAPI to enhance code reuse and consistency.
 */

import type { TriggerTemplate, TriggerReference } from "@wf-agent/types";
import { CrudResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import { isSuccess, getData } from "../../../shared/types/execution-result.js";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";
import type { Timestamp, UnregisterOptions } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

/**
 * Trigger Template Filter
 */
export interface TriggerTemplateFilter {
  /** Template name (fuzzy matching is supported) */
  name?: string;
  /** Keyword search */
  keyword?: string;
  /** Trigger Type */
  triggerType?: string;
  /** Classification */
  category?: string;
  /** Tag array */
  tags?: string[];
}

/**
 * Trigger Template Summary
 */
export interface TriggerTemplateSummary {
  /** Template Name */
  name: string;
  /** Template Description */
  description?: string;
  /** Classification */
  category?: string;
  /** Tag array */
  tags?: string[];
  /** Creation time */
  createdAt: Timestamp;
  /** Update time */
  updatedAt: Timestamp;
}

/**
 * Trigger Template Registry API Class
 *
 * Reconstruction Notes:
 * - Inherit from GenericResourceAPI to reuse common CRUD operations.
 * - Implement all abstract methods to adapt to TriggerTemplateRegistry.
 * - Retain all existing API methods to maintain backward compatibility.
 * - Add enhanced features such as caching, logging, and validation.
 */
export class TriggerTemplateRegistryAPI extends CrudResourceAPI<
  TriggerTemplate,
  string,
  TriggerTemplateFilter
> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  /**
   * Get a single trigger template
   * @param id The name of the trigger template
   * @returns The trigger template; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<TriggerTemplate | null> {
    const template = this.dependencies.getTriggerTemplateRegistry().get(id);
    return template || null;
  }

  /**
   * Get all trigger templates
   * @returns Array of trigger templates
   */
  protected async getAllResources(): Promise<TriggerTemplate[]> {
    return this.dependencies.getTriggerTemplateRegistry().list();
  }

  /**
   * Create a trigger template
   * @param resource Trigger template
   */
  protected async createResource(resource: TriggerTemplate): Promise<void> {
    this.dependencies.getTriggerTemplateRegistry().register(resource);
  }

  /**
   * Update trigger template
   * @param id Trigger template name
   * @param updates Update content
   */
  protected async updateResource(id: string, updates: Partial<TriggerTemplate>): Promise<void> {
    this.dependencies.getTriggerTemplateRegistry().update(id, updates);
  }

  /**
   * Delete trigger template
   * @param id Trigger template name
   */
  protected async deleteResource(id: string): Promise<void> {
    // Check if any workflow references this trigger template
    const referencingWorkflows = await this.findReferencingWorkflows(id);

    if (referencingWorkflows.length > 0) {
      throw new ConfigurationValidationError(
        `Cannot delete trigger template '${id}': it is referenced by workflows: ${referencingWorkflows.join(", ")}`,
        {
          configType: "trigger",
          configPath: "template.delete.referenced",
        },
      );
    }

    this.dependencies.getTriggerTemplateRegistry().unregister(id);
  }

  /**
   * Delete trigger template with options
   * @param id Trigger template name
   * @param options Deletion options
   */
  async deleteWithOptions(id: string, options?: UnregisterOptions): Promise<void> {
    const shouldCheck = options?.checkReferences !== false;

    if (shouldCheck) {
      const referencingWorkflows = await this.findReferencingWorkflows(id);

      if (referencingWorkflows.length > 0 && !options?.force) {
        throw new ConfigurationValidationError(
          `Cannot delete trigger template '${id}': it is referenced by workflows: ${referencingWorkflows.join(", ")}`,
          {
            configType: "trigger",
            configPath: "template.delete.referenced",
          },
        );
      }

      if (options?.force && referencingWorkflows.length > 0) {
        // Log warning but proceed with deletion
        console.warn(
          `Force deleting trigger template '${id}' with ${referencingWorkflows.length} active references: ${referencingWorkflows.join(", ")}`,
        );
      }
    }

    this.dependencies.getTriggerTemplateRegistry().unregister(id, options);
  }

  /**
   * Find workflows that reference this trigger template
   * @param templateName Trigger template name
   * @returns Array of workflow IDs that reference this template
   */
  private async findReferencingWorkflows(templateName: string): Promise<string[]> {
    const workflowRegistry = this.dependencies.getWorkflowRegistry();
    const allSummaries = workflowRegistry.list();

    const referencingWorkflows: string[] = [];

    for (const summary of allSummaries) {
      // Get full workflow definition to check triggers
      const workflow = workflowRegistry.get(summary.id);
      if (!workflow) {
        continue;
      }

      // Check if workflow has triggers that reference this template
      if (workflow.triggers) {
        for (const trigger of workflow.triggers) {
          // Check if it's a TriggerReference with templateName
          if (this.isTriggerReference(trigger) && trigger.templateName === templateName) {
            referencingWorkflows.push(workflow.id);
            break;
          }
        }
      }
    }

    return referencingWorkflows;
  }

  /**
   * Type guard to check if a trigger is a TriggerReference
   * @param trigger Trigger or TriggerReference
   * @returns True if it's a TriggerReference
   */
  private isTriggerReference(trigger: unknown): trigger is TriggerReference {
    return (
      typeof trigger === "object" &&
      trigger !== null &&
      "templateName" in trigger &&
      typeof (trigger as TriggerReference).templateName === "string"
    );
  }

  /**
   * Check if the trigger template can be safely deleted
   * @param templateName Trigger template name
   * @param options Deletion options
   * @returns Whether the template can be deleted and detailed information
   */
  async canSafelyDelete(
    templateName: string,
    options?: UnregisterOptions,
  ): Promise<{ canDelete: boolean; details: string; referencingWorkflows: string[] }> {
    const referencingWorkflows = await this.findReferencingWorkflows(templateName);

    if (referencingWorkflows.length === 0) {
      return {
        canDelete: true,
        details: "No references found",
        referencingWorkflows: [],
      };
    }

    if (options?.force) {
      return {
        canDelete: true,
        details: `Force deleting trigger template with ${referencingWorkflows.length} active references: ${referencingWorkflows.join(", ")}`,
        referencingWorkflows,
      };
    }

    return {
      canDelete: false,
      details: `Cannot delete trigger template: it is referenced by ${referencingWorkflows.length} workflows: ${referencingWorkflows.join(", ")}. Use force=true to override, or check references first.`,
      referencingWorkflows,
    };
  }

  /**
   * Apply filter conditions
   * @param resources Array of trigger templates
   * @param filter Filter criteria
   * @returns Array of trigger templates after filtering
   */
  protected override applyFilter(
    resources: TriggerTemplate[],
    filter: TriggerTemplateFilter,
  ): TriggerTemplate[] {
    let templates = resources;

    if (filter.keyword) {
      templates = this.dependencies.getTriggerTemplateRegistry().search(filter.keyword);
    }

    if (filter.category) {
      templates = templates.filter(t => t.metadata?.["category"] === filter.category);
    }

    if (filter.tags && filter.tags.length > 0) {
      templates = templates.filter(t => {
        const templateTags = (t.metadata?.["tags"] as string[]) || [];
        return filter.tags!.every(tag => templateTags.includes(tag));
      });
    }

    if (filter.name) {
      templates = templates.filter(t => t.name === filter.name);
    }

    return templates;
  }

  /**
   * Get a list of trigger template summaries
   * @param filter (optional) Filter criteria
   * @returns Array of trigger template summaries
   */
  async getTemplateSummaries(filter?: TriggerTemplateFilter): Promise<TriggerTemplateSummary[]> {
    const result = await this.getAll(filter);
    if (!isSuccess(result)) {
      return [];
    }

    const templates = getData(result) || [];
    return templates.map((template: TriggerTemplate) => {
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
   * Search Trigger Template
   * @param keyword Search keyword
   * @returns Array of matching trigger templates
   */
  searchTemplates(keyword: string): TriggerTemplate[] {
    return this.dependencies.getTriggerTemplateRegistry().search(keyword);
  }

  /**
   * Export Trigger Template
   * @param name The name of the trigger template
   * @returns A JSON string
   * @throws NotFoundError If the trigger template does not exist
   */
  exportTemplate(name: string): string {
    return this.dependencies.getTriggerTemplateRegistry().export(name);
  }

  /**
   * Import trigger template
   * @param json JSON string
   * @returns Trigger template name
   * @throws ValidationError If the JSON is invalid or the trigger configuration is invalid
   */
  importTemplate(json: string): string {
    return this.dependencies.getTriggerTemplateRegistry().import(json);
  }

  /**
   * Batch import trigger templates
   * @param jsons: An array of JSON strings
   * @returns: An array of trigger template names
   */
  importTemplates(jsons: string[]): string[] {
    const names: string[] = [];
    for (const json of jsons) {
      names.push(this.importTemplate(json));
    }
    return names;
  }

  /**
   * Batch export of trigger templates
   * @param names Array of trigger template names
   * @returns Array of JSON strings
   */
  exportTemplates(names: string[]): string[] {
    const jsons: string[] = [];
    for (const name of names) {
      jsons.push(this.exportTemplate(name));
    }
    return jsons;
  }
}

/**
 * Global Trigger Template Registry API Instance
 */
// Remove global instances; they will be managed by APIFactory.
