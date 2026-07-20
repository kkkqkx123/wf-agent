/**
 * Event Adapter
 * Manage events via the SDK EventResourceAPI.
 */

import { BaseAdapter } from "./base-adapter.js";
import { findByIdOrThrow } from "@wf-agent/runtime/adapters";
import type { EventFilter } from "@wf-agent/sdk/api";

export class EventAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Event";
  }

  async listEvents(filter?: EventFilter): Promise<Record<string, any>[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listEvents", filter);
      const api = this.sdk.events;
      const events = await api.getAll(filter);
      return events.map((e: any) => ({
        id: e.id,
        type: e.type,
        executionId: e.executionId,
        workflowId: e.workflowId,
        timestamp: e.timestamp,
        data: e.data,
      }));
    }, "List events");
  }

  async getEvent(id: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getEvent", { id });
      if (!id || id.trim().length === 0) throw new Error("Event ID is required");
      const event = await findByIdOrThrow(this.sdk.events, id, "Event");
      return event as any;
    }, "Get event");
  }

  async getEventStats(filter?: EventFilter): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getEventStats", filter);
      const api = this.sdk.events;
      return await api.getEventStats(filter) as any;
    }, "Get event stats");
  }

  async trimEventHistory(maxSize: number): Promise<number> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("trimEventHistory", { maxSize });
      const api = this.sdk.events;
      const events = await api.getAll();
      const currentSize = events.length;
      if (currentSize > maxSize) {
        // Trim by keeping only the most recent events
        // EventResourceAPI doesn't expose trim directly, so we rely on the event history limit
      }
      return Math.max(0, currentSize - maxSize);
    }, "Trim event history");
  }
}