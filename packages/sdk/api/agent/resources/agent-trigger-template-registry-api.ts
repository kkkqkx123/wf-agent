/**
 * Agent Trigger Template Registry API
 * Provides a user-friendly API interface for managing agent trigger templates.
 * Follows the same pattern as Workflow's TriggerTemplateRegistryAPI.
 */

import type { Timestamp } from "@wf-agent/types";
import type { TriggerTemplate, TriggerReference } from "@wf-agent/types";
import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { DeleteCheckResult } from "../../../shared/registry/types.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "AgentTriggerTemplateRegistryAPI" });

/**
 * Agent Trigger Template Filter
 */
export interface AgentTriggerTemplateFilter {
  /** Template name (fuzzy matching supported) */
  name?: string;
  /** Trigger type filter */
  triggerType?: string;
  /** Category filter */
  category?: string;
  /** Tags filter */
  tags?: string[];
}

/**
 * Agent Trigger Template Summary
 */
export interface AgentTriggerTemplateSummary {
  /** Template ID/name */
  id: string;
  /** Template name */
  name: string;
  /** Trigger type */
  triggerType: "event" | "condition" | "schedule";
  /** Template description */
  description?: string;
  /** Category */
  category?: string;
  /** Tags */
  tags?: string[];
  /** Creation time */
  createdAt: Timestamp;
  /** Update time */
  updatedAt: Timestamp;
}

/**
 * Agent Trigger Template Definition
 * Wraps the underlying TriggerTemplate with agent-specific fields.
 * The `id` field is mapped from the underlying TriggerTemplate's `name`.
 */
export interface AgentTriggerTemplate {
  /** Template ID */
  id: string;
  /** Template name */
  name: string;
  /** Trigger type */
  type: "event" | "condition" | "schedule";
  /** Trigger condition expression */
  condition?: string;
  /** Event name (for event-based triggers) */
  eventName?: string;
  /** Default action type */
  actionType: "pause" | "stop" | "checkpoint" | "custom";
  /** Action configuration */
  actionConfig?: Record<string, unknown>;
  /** Template description */
  description?: string;
  /** Category for organization */
  category?: string;
  /** Tags for searching */
  tags?: string[];
  /** Creation time */
  createdAt: Timestamp;
  /** Update time */
  updatedAt: Timestamp;
  /** Whether template is enabled */
  enabled: boolean;
}

/**
 * Convert a TriggerTemplate from the shared registry to an AgentTriggerTemplate.
 * The shared TriggerTemplate stores trigger config in `condition` and `action` objects,
 * while AgentTriggerTemplate flattens key fields at the top level.
 */
function toAgentTriggerTemplate(template: TriggerTemplate): AgentTriggerTemplate {
  const metadata = template.metadata ?? {};
  const triggerType = inferTriggerType(template);
  return {
    id: template.name,
    name: template.name,
    type: triggerType,
    condition: extractConditionExpression(template),
    eventName: template.condition?.eventName,
    actionType: inferActionType(template),
    actionConfig: template.action?.parameters as unknown as Record<string, unknown> | undefined,
    description: template.description,
    category: (metadata as Record<string, unknown>)["category"] as string | undefined,
    tags: (metadata as Record<string, unknown>)["tags"] as string[] | undefined,
    createdAt: template.createdAt,
    updatedAt: template.updatedAt,
    enabled: template.enabled !== false,
  };
}

/**
 * Extract the condition expression string from the shared TriggerTemplate's condition.
 * Falls back to JSON.stringify for non-expression conditions.
 */
function extractConditionExpression(template: TriggerTemplate): string | undefined {
  const cond = template.condition?.condition;
  if (!cond) return undefined;
  if (typeof cond === "object" && "expression" in cond) {
    return (cond as { expression: string }).expression;
  }
  if (typeof cond === "object" && "script" in cond) {
    return (cond as { script: string }).script;
  }
  return JSON.stringify(cond);
}

/**
 * Infer the trigger type from the shared TriggerTemplate's condition event type.
 */
function inferTriggerType(template: TriggerTemplate): "event" | "condition" | "schedule" {
  const eventType = template.condition?.eventType as string;
  if (eventType && eventType.includes("SCHEDULE")) {
    return "schedule";
  }
  if (template.condition?.eventName || (eventType && eventType.includes("EVENT"))) {
    return "event";
  }
  return "condition";
}

/**
 * Infer the action type from the shared TriggerTemplate's action type.
 */
function inferActionType(template: TriggerTemplate): "pause" | "stop" | "checkpoint" | "custom" {
  const actionType = template.action?.type as string;
  if (actionType === "pause_workflow_execution") {
    return "pause";
  }
  if (actionType === "stop_workflow_execution") {
    return "stop";
  }
  if (actionType === "checkpoint") {
    return "checkpoint";
  }
  return "custom";
}

/**
 * Agent Trigger Template Registry API
 */
export class AgentTriggerTemplateRegistryAPI extends SimplifiedCrudResourceAPI<
  AgentTriggerTemplate,
  string,
  AgentTriggerTemplateFilter
> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  /**
   * Get a single trigger template by ID
   * @param id Template ID/name
   * @returns The trigger template; returns null if it does not exist
   */
  protected async getResource(id: string): Promise<AgentTriggerTemplate | null> {
    const template = this.dependencies.getTriggerTemplateRegistry().get(id);
    if (!template) return null;
    return toAgentTriggerTemplate(template);
  }

  /**
   * Get all trigger templates
   * @returns Array of all trigger templates
   */
  protected async getAllResources(): Promise<AgentTriggerTemplate[]> {
    const templates = this.dependencies.getTriggerTemplateRegistry().list();
    return templates.map(toAgentTriggerTemplate);
  }

  /**
   * Create a new trigger template
   * @param resource Template to create
   */
  protected async createResource(resource: AgentTriggerTemplate): Promise<void> {
    this.dependencies.getTriggerTemplateRegistry().register(resource as unknown as TriggerTemplate);
  }

  /**
   * Update an existing trigger template
   * @param id Template ID
   * @param updates Updated fields
   */
  protected async updateResource(
    id: string,
    updates: Partial<AgentTriggerTemplate>
  ): Promise<void> {
    this.dependencies.getTriggerTemplateRegistry().update(id, updates as unknown as Partial<TriggerTemplate>);
  }

  /**
   * Delete a trigger template
   * @param id Template ID to delete
   */
  protected async deleteResource(id: string): Promise<void> {
    this.dependencies.getTriggerTemplateRegistry().unregister(id);
  }

  /**
   * Query trigger templates with filters
   * @param filter Query filter
   * @returns Array of matching trigger templates
   */
  async query(filter: AgentTriggerTemplateFilter): Promise<AgentTriggerTemplate[]> {
    const allTemplates = await this.getAllResources();

    let results = allTemplates;

    if (filter.name) {
      const nameLower = filter.name.toLowerCase();
      results = results.filter((t) => t.name.toLowerCase().includes(nameLower));
    }

    if (filter.triggerType) {
      results = results.filter((t) => t.type === filter.triggerType);
    }

    if (filter.category) {
      results = results.filter((t) => t.category === filter.category);
    }

    if (filter.tags && filter.tags.length > 0) {
      results = results.filter((t) =>
        t.tags ? filter.tags!.some((tag) => t.tags!.includes(tag)) : false
      );
    }

    return results;
  }

  /**
   * Get trigger templates by type
   * @param triggerType Trigger type to filter by
   * @returns Array of templates matching the type
   */
  async queryByType(triggerType: "event" | "condition" | "schedule"): Promise<AgentTriggerTemplate[]> {
    return this.query({ triggerType });
  }

  /**
   * Get trigger templates by category
   * @param category Category to filter by
   * @returns Array of templates in the category
   */
  async queryByCategory(category: string): Promise<AgentTriggerTemplate[]> {
    return this.query({ category });
  }

  /**
   * Get trigger templates by tags
   * @param tags Tags to filter by
   * @returns Array of templates with any of the tags
   */
  async queryByTags(tags: string[]): Promise<AgentTriggerTemplate[]> {
    return this.query({ tags });
  }

  /**
   * Get trigger template summaries.
   * @param filter - Optional filter criteria
   * @returns Array of trigger template summaries
   */
  async getTemplateSummaries(filter?: AgentTriggerTemplateFilter): Promise<AgentTriggerTemplateSummary[]> {
    const templates = await this.getAllResources();

    let results = templates;

    if (filter) {
      if (filter.name) {
        const nameLower = filter.name.toLowerCase();
        results = results.filter((t) => t.name.toLowerCase().includes(nameLower));
      }
      if (filter.triggerType) {
        results = results.filter((t) => t.type === filter.triggerType);
      }
      if (filter.category) {
        results = results.filter((t) => t.category === filter.category);
      }
      if (filter.tags && filter.tags.length > 0) {
        results = results.filter((t) =>
          t.tags ? filter.tags!.some((tag) => t.tags!.includes(tag)) : false,
        );
      }
    }

    return results.map((t) => ({
      id: t.id,
      name: t.name,
      triggerType: t.type,
      description: t.description,
      category: t.category,
      tags: t.tags,
      createdAt: t.createdAt,
      updatedAt: t.updatedAt,
    }));
  }

  /**
   * Search trigger templates.
   * @param keyword - Search keyword
   * @returns Array of matching trigger templates
   */
  searchTemplates(keyword: string): AgentTriggerTemplate[] {
    const templates = this.dependencies.getTriggerTemplateRegistry().search(keyword);
    return templates.map(toAgentTriggerTemplate);
  }

  /**
   * Export a trigger template as JSON.
   * @param name - Trigger template name
   * @returns JSON string
   */
  exportTemplate(name: string): string {
    return this.dependencies.getTriggerTemplateRegistry().export(name);
  }

  /**
   * Import a trigger template from JSON.
   * @param json - JSON string
   * @returns Trigger template name
   */
  importTemplate(json: string): string {
    return this.dependencies.getTriggerTemplateRegistry().import(json);
  }

  /**
   * Batch import trigger templates.
   * @param jsons - Array of JSON strings
   * @returns Array of trigger template names
   */
  importTemplates(jsons: string[]): string[] {
    const names: string[] = [];
    for (const json of jsons) {
      names.push(this.importTemplate(json));
    }
    return names;
  }

  /**
   * Batch export trigger templates.
   * @param names - Array of trigger template names
   * @returns Array of JSON strings
   */
  exportTemplates(names: string[]): string[] {
    const jsons: string[] = [];
    for (const name of names) {
      jsons.push(this.exportTemplate(name));
    }
    return jsons;
  }

  /**
   * Delete trigger template with options.
   * @param id - Trigger template name
   * @param options - Deletion options
   */
  override async deleteWithOptions(
    id: string,
    options?: { force?: boolean; checkReferences?: boolean },
  ): Promise<void> {
    const shouldCheck = options?.checkReferences !== false;

    if (shouldCheck) {
      const check = await this.checkDeleteReferences(id);

      if (!check.canDelete && !options?.force) {
        throw new ConfigurationValidationError(
          `Cannot delete trigger template '${id}': ${check.details}`,
          {
            configType: "trigger",
            configPath: "template.delete.referenced",
          },
        );
      }

      if (options?.force && !check.canDelete) {
        const refNames = check.references.map((r) => r.sourceId).join(", ");
        logger.warn(
          `Force deleting trigger template '${id}' with ${check.references.length} active references: ${refNames}`,
        );
      }
    }

    this.dependencies.getTriggerTemplateRegistry().unregister(id);
  }

  /**
   * Check if the trigger template can be safely deleted.
   * @param templateName - Trigger template name
   * @param options - Deletion options
   * @returns Whether the template can be deleted and detailed information
   */
  override async canSafelyDelete(
    templateName: string,
    options?: { force?: boolean },
  ): Promise<DeleteCheckResult> {
    const result = await this.checkDeleteReferences(templateName);

    if (result.canDelete) {
      return result;
    }

    if (options?.force) {
      return {
        canDelete: true,
        details: `Force deleting trigger template with ${result.references.length} active references: ${result.references.map((r) => r.sourceId).join(", ")}`,
        references: result.references,
      };
    }

    return result;
  }

  /**
   * Check references for safe deletion.
   * Finds agent loops that reference this trigger template.
   * @param id - Trigger template name
   * @returns Delete check result
   */
  protected override async checkDeleteReferences(id: string): Promise<DeleteCheckResult> {
    const referencingAgentLoops = await this.findReferencingAgentLoops(id);

    const references: DeleteCheckResult["references"] = referencingAgentLoops.map((loopId) => ({
      type: "agent_loop_trigger" as const,
      sourceId: loopId,
      details: "Agent loop has trigger referencing this template",
    }));

    return {
      canDelete: references.length === 0,
      details:
        references.length === 0
          ? "No references found"
          : `Referenced by ${references.length} agent loop(s): ${referencingAgentLoops.join(", ")}`,
      references,
    };
  }

  /**
   * Find agent loops that reference this trigger template.
   * @param templateName - Trigger template name
   * @returns Array of agent loop IDs that reference this template
   */
  private async findReferencingAgentLoops(templateName: string): Promise<string[]> {
    const agentLoopRegistry = this.dependencies.getAgentLoopRegistry();
    const allAgentLoops = agentLoopRegistry.getAll();

    const referencingLoops: string[] = [];

    for (const agentLoop of allAgentLoops) {
      const triggers = agentLoop.config.triggers || [];
      for (const trigger of triggers) {
        if (this.isTriggerReference(trigger) && trigger.templateName === templateName) {
          referencingLoops.push(agentLoop.id);
          break;
        }
      }
    }

    return referencingLoops;
  }

  /**
   * Type guard to check if a trigger is a TriggerReference.
   */
  private isTriggerReference(trigger: unknown): trigger is TriggerReference {
    return (
      typeof trigger === "object" &&
      trigger !== null &&
      "templateName" in trigger &&
      typeof (trigger as TriggerReference).templateName === "string"
    );
  }
}