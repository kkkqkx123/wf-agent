/**
 * Trigger Adapter
 * Encapsulates SDK API calls related to triggers
 */

import { BaseAdapter } from "./base-adapter.js";
import type { Trigger, TriggerTemplateFilter, TriggerTemplate, TriggerTemplateSummary } from "@wf-agent/types";
import type { UnregisterOptions } from "@wf-agent/types";
import { CLINotFoundError } from "../types/cli-types.js";
import { loadTriggerTemplateConfig } from "@wf-agent/config-processor";

/**
 * Trigger Adapter
 */
export class TriggerAdapter extends BaseAdapter {
  /**
   * List all triggers
   */
  async listTriggers(filter?: TriggerTemplateFilter): Promise<Trigger[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggers;
      return await api.getAll(filter);
    }, "List triggers");
  }

  /**
   * Get trigger details
   */
  async getTrigger(id: string): Promise<Trigger> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggers;
      const trigger = await api.get(id);

      if (!trigger) {
        throw new CLINotFoundError(`Trigger not found: ${id}`, "Trigger", id);
      }

      return trigger as Trigger;
    }, "Get trigger");
  }

  /**
   * Enable trigger
   */
  async enableTrigger(executionId: string, triggerId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggers;
      await api.enableTrigger(executionId, triggerId);
      this.output.infoLog(`Trigger enabled: ${triggerId}`);
    }, "Enable trigger");
  }

  /**
   * Disable trigger
   */
  async disableTrigger(executionId: string, triggerId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggers;
      await api.disableTrigger(executionId, triggerId);
      this.output.infoLog(`Trigger disabled: ${triggerId}`);
    }, "Disable trigger");
  }

  /**
   * List triggers by workflow execution ID
   */
  async listTriggersByWorkflowExecution(executionId: string): Promise<Trigger[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggers;
      const result = await api.getWorkflowExecutionTriggers(executionId);
      return result;
    }, "List workflow execution triggers");
  }

  // ========================================================================
  // Trigger Template Methods
  // ========================================================================

  /**
   * Register a trigger template from a file
   */
  async registerTriggerTemplateFromFile(filePath: string, parameters?: Record<string, unknown>): Promise<TriggerTemplate> {
    return this.executeWithErrorHandling(async () => {
      const parsed = await loadTriggerTemplateConfig(filePath);
      let template = parsed.config;

      // Apply runtime parameters if provided
      if (parameters) {
        template = { ...template, ...parameters };
      }

      const api = this.sdk.triggerTemplates;
      const result = await api.create(template);

      if (result.result.isErr()) {
        throw new Error(result.result.error.message);
      }

      this.logOperation(`Trigger template registered: ${template.name}`);
      return template;
    }, "Register trigger template");
  }

  /**
   * List all trigger templates
   */
  async listTriggerTemplates(filter?: TriggerTemplateFilter): Promise<TriggerTemplateSummary[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggerTemplates;
      return await api.getTemplateSummaries(filter);
    }, "List trigger templates");
  }

  /**
   * Get a trigger template by name
   */
  async getTriggerTemplate(name: string): Promise<TriggerTemplate> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggerTemplates;
      const template = await api.get(name);

      if (!template) {
        throw new CLINotFoundError(`Trigger template not found: ${name}`, "TriggerTemplate", name);
      }

      return template;
    }, "Get trigger template");
  }

  /**
   * Delete a trigger template
   */
  async deleteTriggerTemplate(name: string, options?: UnregisterOptions): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggerTemplates;
      await api.deleteWithOptions(name, options);
      this.logOperation(`Trigger template deleted: ${name}`);
    }, "Delete trigger template");
  }

  /**
   * Export a trigger template as JSON string
   */
  async exportTriggerTemplate(name: string): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggerTemplates;
      const json = api.exportTemplate(name);
      return json;
    }, "Export trigger template");
  }
}
