/**
 * Event Adapter
 * Encapsulates SDK API calls related to events
 */

import { BaseAdapter } from "./base-adapter.js";
import type { BaseEvent } from "@wf-agent/types";
import type { EventFilter } from "@wf-agent/sdk";
import { CLINotFoundError, CLIAPIError } from "../types/cli-types.js";

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
      const events = (result as any).data || result;
      return events as BaseEvent[];
    }, "List events");
  }

  /**
   * Get event details
   */
  async getEvent(id: string): Promise<BaseEvent> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.events;
      const result = await api.get(id);
      const event = (result as any).data || result;
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
      if (typeof api.dispatch === "function") {
        await api.dispatch(event);
        this.output.infoLog(`Event dispatched: ${event.type}`);
      } else {
        throw new CLIAPIError(
          "The current version of the SDK does not support event dispatch.",
          501,
          "events/dispatch",
        );
      }
    }, "Dispatch event");
  }

  /**
   * Trim event history
   */
  async trimEventHistory(maxSize: number): Promise<number> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.events as any;
      if (typeof api.trimEventHistory === "function") {
        const removed = await api.trimEventHistory(maxSize);
        this.output.infoLog(`Trimmed ${removed} event(s) from history`);
        return removed;
      } else {
        throw new CLIAPIError(
          "The current version of the SDK does not support trimming event history.",
          501,
          "events/trimEventHistory",
        );
      }
    }, "Trim event history");
  }
}
