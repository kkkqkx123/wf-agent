/**
 * Trigger Adapter
 * Manage triggers via the SDK TriggerResourceAPI.
 */

import { BaseAdapter } from "./base-adapter.js";
import { findByIdOrThrow } from "@wf-agent/runtime/adapters";

export class TriggerAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Trigger";
  }

  async listTriggers(filter?: Record<string, unknown>): Promise<Record<string, any>[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listTriggers", filter);
      const api = this.sdk.triggers;
      const triggers = await api.getAll(filter as any);
      return triggers.map((t: any) => ({
        id: t.id,
        name: t.name || t.id,
        type: t.triggerType,
        workflowId: t.workflowId,
        enabled: t.enabled,
        eventType: t.eventType,
      }));
    }, "List triggers");
  }

  async getTrigger(id: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getTrigger", { id });
      if (!id || id.trim().length === 0) throw new Error("Trigger ID is required");
      const trigger = await findByIdOrThrow(this.sdk.triggers, id, "Trigger");
      return trigger as any;
    }, "Get trigger");
  }

  async enableTrigger(executionId: string, triggerId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("enableTrigger", { executionId, triggerId });
      const api = this.sdk.triggers;
      await api.enableTrigger(executionId, triggerId);
    }, "Enable trigger");
  }

  async disableTrigger(executionId: string, triggerId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("disableTrigger", { executionId, triggerId });
      const api = this.sdk.triggers;
      await api.disableTrigger(executionId, triggerId);
    }, "Disable trigger");
  }

  async listTriggersByWorkflowExecution(executionId: string): Promise<Record<string, any>[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listTriggersByWorkflowExecution", { executionId });
      const api = this.sdk.triggers;
      const triggers = await api.getAll({ executionId } as any);
      return triggers.map((t: any) => ({
        id: t.id,
        name: t.name || t.id,
        type: t.triggerType,
        workflowId: t.workflowId,
        enabled: t.enabled,
        eventType: t.eventType,
      }));
    }, "List triggers by execution");
  }
}