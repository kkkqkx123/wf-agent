/**
 * Agent Template Registry Adapter
 * Encapsulates agent-level template registry operations for CLI use
 *
 * Covers three registries:
 * - AgentTemplateRegistryAPI (agent config templates)
 * - AgentTriggerTemplateRegistryAPI (trigger templates)
 * - AgentHookTemplateRegistryAPI (hook templates)
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  AgentTemplateRegistryAPI,
  AgentTriggerTemplateRegistryAPI,
  AgentHookTemplateRegistryAPI,
  type AgentTemplateFilter,
  type AgentTriggerTemplateFilter,
  type AgentTriggerTemplateSummary,
  type AgentHookTemplateFilter,
  type AgentHookTemplateSummary,
} from "@wf-agent/sdk/api";
import type {
  AgentTemplate,
  HookTemplate,
} from "@wf-agent/types";

/**
 * Agent Template Registry Adapter
 * Provides CLI-friendly access to agent-level template registries
 */
export class AgentTemplateRegistryAdapter extends BaseAdapter {
  private templateApi: AgentTemplateRegistryAPI;
  private triggerTemplateApi: AgentTriggerTemplateRegistryAPI;
  private hookTemplateApi: AgentHookTemplateRegistryAPI;

  constructor() {
    super();
    this.templateApi = this.sdk.agentTemplates;
    this.triggerTemplateApi = this.sdk.agentTriggerTemplates;
    this.hookTemplateApi = this.sdk.agentHookTemplates;
  }

  // ====================================================================
  // Agent Template (config templates)
  // ====================================================================

  async listTemplates(filter?: AgentTemplateFilter): Promise<AgentTemplate[]> {
    return this.executeWithErrorHandling(async () => {
      return this.templateApi.getAll(filter);
    }, "List agent templates");
  }

  async getTemplate(id: string): Promise<AgentTemplate | null> {
    return this.executeWithErrorHandling(async () => {
      return this.templateApi.get(id);
    }, `Get agent template "${id}"`);
  }

  async createTemplate(template: AgentTemplate): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.templateApi.create(template);
    }, `Create agent template "${template.templateName ?? template.id}"`);
  }

  async deleteTemplate(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.templateApi.delete(id);
    }, `Delete agent template "${id}"`);
  }

  async queryByCategory(category: string): Promise<AgentTemplate[]> {
    return this.executeWithErrorHandling(async () => {
      return this.templateApi.queryByCategory(category);
    }, `Query agent templates by category "${category}"`);
  }

  async queryByTags(tags: string[]): Promise<AgentTemplate[]> {
    return this.executeWithErrorHandling(async () => {
      return this.templateApi.queryByTags(tags);
    }, `Query agent templates by tags`);
  }

  // ====================================================================
  // Agent Trigger Templates
  // ====================================================================

  async listTriggerTemplates(filter?: AgentTriggerTemplateFilter) {
    return this.executeWithErrorHandling(async () => {
      return this.triggerTemplateApi.getAll(filter);
    }, "List trigger templates");
  }

  async getTriggerTemplate(id: string) {
    return this.executeWithErrorHandling(async () => {
      return this.triggerTemplateApi.get(id);
    }, `Get trigger template "${id}"`);
  }

  async createTriggerTemplate(template: any) {
    return this.executeWithErrorHandling(async () => {
      await this.triggerTemplateApi.create(template);
    }, `Create trigger template "${template["name"]}"`);
  }

  async deleteTriggerTemplate(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.triggerTemplateApi.delete(id);
    }, `Delete trigger template "${id}"`);
  }

  async getTriggerTemplateSummaries(
    filter?: AgentTriggerTemplateFilter,
  ): Promise<AgentTriggerTemplateSummary[]> {
    return this.executeWithErrorHandling(async () => {
      return this.triggerTemplateApi.getTemplateSummaries(filter);
    }, "Get trigger template summaries");
  }

  async queryTriggerByType(
    triggerType: "event" | "condition" | "schedule",
  ) {
    return this.executeWithErrorHandling(async () => {
      return this.triggerTemplateApi.queryByType(triggerType);
    }, `Query trigger templates by type "${triggerType}"`);
  }

  // ====================================================================
  // Agent Hook Templates
  // ====================================================================

  async listHookTemplates(filter?: AgentHookTemplateFilter): Promise<HookTemplate[]> {
    return this.executeWithErrorHandling(async () => {
      return this.hookTemplateApi.getAll(filter);
    }, "List hook templates");
  }

  async getHookTemplate(id: string): Promise<HookTemplate | null> {
    return this.executeWithErrorHandling(async () => {
      return this.hookTemplateApi.get(id);
    }, `Get hook template "${id}"`);
  }

  async createHookTemplate(template: HookTemplate): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.hookTemplateApi.create(template);
    }, `Create hook template "${template.name}"`);
  }

  async deleteHookTemplate(id: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.hookTemplateApi.delete(id);
    }, `Delete hook template "${id}"`);
  }

  async getHookTemplateSummaries(
    filter?: AgentHookTemplateFilter,
  ): Promise<AgentHookTemplateSummary[]> {
    return this.executeWithErrorHandling(async () => {
      return this.hookTemplateApi.getTemplateSummaries(filter);
    }, "Get hook template summaries");
  }

  async queryHookByType(hookType: string): Promise<HookTemplate[]> {
    return this.executeWithErrorHandling(async () => {
      return this.hookTemplateApi.queryByHookType(hookType);
    }, `Query hook templates by type "${hookType}"`);
  }
}
