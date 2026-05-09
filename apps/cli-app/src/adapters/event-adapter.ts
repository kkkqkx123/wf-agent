/**
 * Event Adapter
 * Encapsulates SDK API calls related to events
 */

import { BaseAdapter } from "./base-adapter.js";
import type { BaseEvent } from "@wf-agent/types";
import type { EventFilter } from "@wf-agent/sdk";
import { CLINotFoundError } from "../types/cli-types.js";
import { getData, isFailure, getError } from "@wf-agent/sdk";

/**
 * Event Adapter
 */
export class EventAdapter extends BaseAdapter {
  /**
   * List all events
   */
  async listEvents(filter?: EventFilter): Promise<BaseEvent[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.events;
      const result = await api.getAll(filter);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result) as BaseEvent[];
    }, "List events");
  }

  /**
   * Get event details
   */
  async getEvent(id: string): Promise<BaseEvent> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.events;
      const result = await api.get(id);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const event = getData(result);
      if (!event) {
        throw new CLINotFoundError(`Event not found: ${id}`, "Event", id);
      }
      
      return event as BaseEvent;
    }, "Get the event");
  }

  /**
   * Get event statistics
   */
  async getEventStats(filter?: EventFilter): Promise<{
    total: number;
    byType: Record<string, number>;
    byExecution: Record<string, number>;
    byWorkflow: Record<string, number>;
  }> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.events;
      const result = await api.getEventStats(filter);
      return result;
    }, "Get event statistics");
  }

  /**
   * Dispatch custom event
   */
  async dispatchEvent(event: BaseEvent): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.events as any;
      
      // Check if dispatch method exists
      if (typeof api.dispatch !== "function") {
        throw new Error("Event dispatch is not supported in the current SDK version");
      }
      
      await api.dispatch(event);
      this.output.infoLog(`Event dispatched: ${event.type}`);
    }, "Dispatch event");
  }

  /**
   * Trim event history
   */
  async trimEventHistory(maxSize: number): Promise<number> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.events as any;
      
      if (typeof api.trimEventHistory !== "function") {
        throw new Error("Trimming event history is not supported in the current SDK version");
      }
      
      const removed = await api.trimEventHistory(maxSize);
      this.output.infoLog(`Trimmed ${removed} event(s) from history`);
      return removed;
    }, "Trim event history");
  }
}
