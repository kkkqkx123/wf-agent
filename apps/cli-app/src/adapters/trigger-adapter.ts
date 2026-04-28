/**
 * Trigger Adapter
 * Encapsulates SDK API calls related to triggers
 */

import { BaseAdapter } from "./base-adapter.js";
import type { Trigger, TriggerTemplateFilter } from "@wf-agent/types";

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
      const result = await api.getAll(filter);
      const triggers = (result as any).data || result;
      return triggers as Trigger[];
    }, "List triggers");
  }

  /**
   * Get trigger details
   */
  async getTrigger(id: string): Promise<Trigger> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggers;
      const result = await api.get(id);
      const trigger = (result as any).data || result;
      return trigger as Trigger;
    }, "Get trigger");
  }

  /**
   * Enable trigger
   */
  async enableTrigger(threadId: string, triggerId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggers;
      await api.enableTrigger(threadId, triggerId);
      this.output.infoLog(`Trigger enabled: ${triggerId}`);
    }, "Enable trigger");
  }

  /**
   * Disable trigger
   */
  async disableTrigger(threadId: string, triggerId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggers;
      await api.disableTrigger(threadId, triggerId);
      this.output.infoLog(`Trigger disabled: ${triggerId}`);
    }, "Disable trigger");
  }

  /**
   * List triggers by thread ID
   */
  async listTriggersByThread(threadId: string): Promise<Trigger[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.triggers;
      const result = await api.getThreadTriggers(threadId);
      return result;
    }, "List thread triggers");
  }
}
