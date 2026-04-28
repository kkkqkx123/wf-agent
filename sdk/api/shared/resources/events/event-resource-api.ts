/**
 * EventResourceAPI - Event Resource Management API
 * Refactored version: Removed invalid memory cache, connected to EventRegistry
 *
 * Design Notes:
 * - Event history is collected in real-time by listening to EventRegistry
 * - Provides event query, statistics, and search functionality
 * - Events cannot be created, updated, or deleted through API
 *
 * Note: This is now a shared API component as events are a cross-module concern
 * used by Graph, Agent, and other modules.
 */

import { ReadonlyResourceAPI } from "../generic-resource-api.js";
import type { BaseEvent, Event, EventType } from "@wf-agent/types";
import type { Timestamp } from "@wf-agent/types";
import { DispatchEventCommand } from "../../operations/events/dispatch-event-command.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import { CommandExecutor } from "../../common/command-executor.js";

/**
 * All event types list
 * Used to listen to all events
 */
const ALL_EVENT_TYPES: EventType[] = [
  // Thread events
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
  // Node events
  "NODE_STARTED",
  "NODE_COMPLETED",
  "NODE_FAILED",
  "NODE_CUSTOM_EVENT",
  // Token events
  "TOKEN_LIMIT_EXCEEDED",
  "TOKEN_USAGE_WARNING",
  // Context compression events
  "CONTEXT_COMPRESSION_REQUESTED",
  "CONTEXT_COMPRESSION_COMPLETED",
  // Message events
  "MESSAGE_ADDED",
  // Tool events
  "TOOL_CALL_STARTED",
  "TOOL_CALL_COMPLETED",
  "TOOL_CALL_FAILED",
  "TOOL_ADDED",
  // Conversation events
  "CONVERSATION_STATE_CHANGED",
  // Error events
  "ERROR",
  // Checkpoint events
  "CHECKPOINT_CREATED",
  "CHECKPOINT_RESTORED",
  "CHECKPOINT_DELETED",
  "CHECKPOINT_FAILED",
  // Subgraph events
  "SUBGRAPH_STARTED",
  "SUBGRAPH_COMPLETED",
  "TRIGGERED_SUBGRAPH_STARTED",
  "TRIGGERED_SUBGRAPH_COMPLETED",
  "TRIGGERED_SUBGRAPH_FAILED",
  // Variable events
  "VARIABLE_CHANGED",
  // User interaction events
  "USER_INTERACTION_REQUESTED",
  "USER_INTERACTION_RESPONDED",
  "USER_INTERACTION_PROCESSED",
  "USER_INTERACTION_FAILED",
  // HumanRelay events
  "HUMAN_RELAY_REQUESTED",
  "HUMAN_RELAY_RESPONDED",
  "HUMAN_RELAY_PROCESSED",
  "HUMAN_RELAY_FAILED",
  // LLM stream events
  "LLM_STREAM_ABORTED",
  "LLM_STREAM_ERROR",
  // Agent events
  "AGENT_CUSTOM_EVENT",
  // Skill events
  "SKILL_LOAD_STARTED",
  "SKILL_LOAD_COMPLETED",
  "SKILL_LOAD_FAILED",
];

/**
 * Event filter
 */
export interface EventFilter {
  /** Event ID list */
  ids?: string[];
  /** Event type */
  eventType?: EventType;
  /** Thread ID */
  threadId?: string;
  /** Workflow ID */
  workflowId?: string;
  /** Node ID */
  nodeId?: string;
  /** Creation time range */
  timestampRange?: { start?: Timestamp; end?: Timestamp };
}

/**
 * Event statistics
 */
export interface EventStats {
  /** Total count */
  total: number;
  /** Statistics by type */
  byType: Record<string, number>;
  /** Statistics by thread */
  byThread: Record<string, number>;
  /** Statistics by workflow */
  byWorkflow: Record<string, number>;
}

/**
 * EventResourceAPI - Event Resource Management API
 *
 * Refactoring Notes:
 * - Removed invalid memory cache
 * - Collects event history by listening to EventRegistry
 * - Provides real-time event query and statistics functionality
 */
export class EventResourceAPI extends ReadonlyResourceAPI<BaseEvent, string, EventFilter> {
  private eventHistory: BaseEvent[] = [];
  private dependencies: APIDependencyManager;
  private executor: CommandExecutor;
  private unsubscribe?: () => void;
  private maxHistorySize: number;

  constructor(dependencies: APIDependencyManager, maxHistorySize: number = 1000) {
    super();
    this.dependencies = dependencies;
    this.executor = new CommandExecutor();
    this.maxHistorySize = maxHistorySize;

    // Setup event listeners
    this.setupEventListeners();
  }

  /**
   * Setup event listeners to collect all events into history
   */
  private setupEventListeners(): void {
    const eventManager = this.dependencies.getEventManager();

    const listeners: Array<() => void> = [];

    // Listen to all event types
    for (const eventType of ALL_EVENT_TYPES) {
      const unsubscribe = eventManager.on(eventType, (event: Event) => {
        this.addEventToHistory(event);
      });
      listeners.push(unsubscribe);
    }

    // Save unsubscribe function
    this.unsubscribe = () => {
      listeners.forEach(unsub => unsub());
    };
  }

  /**
   * Add event to history (internal method)
   */
  private addEventToHistory(event: BaseEvent): void {
    this.eventHistory.push(event);

    // Auto-trim history
    if (this.eventHistory.length > this.maxHistorySize) {
      this.eventHistory = this.eventHistory.slice(-this.maxHistorySize);
    }
  }

  /**
   * Cleanup resources
   */
  public dispose(): void {
    if (this.unsubscribe) {
      this.unsubscribe();
      this.unsubscribe = undefined;
    }
    this.eventHistory = [];
  }

  // ============================================================================
  // Implement abstract methods
  // ============================================================================

  /**
   * Get single event
   * @param id Event ID
   * @returns Event object, or null if not found
   */
  protected async getResource(id: string): Promise<BaseEvent | null> {
    return (
      this.eventHistory.find(
        event => `${event.type}-${event.threadId}-${event.timestamp}` === id,
      ) || null
    );
  }

  /**
   * Get all events
   * @returns Event array
   */
  protected async getAllResources(): Promise<BaseEvent[]> {
    return [...this.eventHistory];
  }

  /**
   * Apply filter conditions
   */
  protected override applyFilter(events: BaseEvent[], filter: EventFilter): BaseEvent[] {
    return events.filter(event => {
      if (filter.eventType && event.type !== filter.eventType) {
        return false;
      }
      if (filter.threadId && event.threadId !== filter.threadId) {
        return false;
      }
      if (filter.workflowId && event.workflowId !== filter.workflowId) {
        return false;
      }
      if (
        filter.nodeId &&
        "nodeId" in event &&
        (event as { nodeId?: string }).nodeId !== filter.nodeId
      ) {
        return false;
      }
      if (filter.timestampRange?.start && event.timestamp < filter.timestampRange.start) {
        return false;
      }
      if (filter.timestampRange?.end && event.timestamp > filter.timestampRange.end) {
        return false;
      }
      return true;
    });
  }

  // ============================================================================
  // Event-specific methods
  // ============================================================================

  /**
   * Dispatch event to system bus
   * @param event Event object
   */
  async dispatch(event: BaseEvent): Promise<void> {
    const command = new DispatchEventCommand({ event: event as Event }, this.dependencies);
    await this.executor.execute(command);
  }

  /**
   * Get event list
   * @param filter Filter conditions
   * @returns Event array
   */
  async getEvents(filter?: EventFilter): Promise<BaseEvent[]> {
    let events = this.eventHistory;

    // Apply filter conditions
    if (filter) {
      events = this.applyFilter(events, filter);
    }

    return events;
  }

  /**
   * Get event statistics
   * @param filter Filter conditions
   * @returns Statistics
   */
  async getEventStats(filter?: EventFilter): Promise<EventStats> {
    let events = this.eventHistory;

    // Apply filter conditions
    if (filter) {
      events = this.applyFilter(events, filter);
    }

    const stats: EventStats = {
      total: events.length,
      byType: {},
      byThread: {},
      byWorkflow: {},
    };

    for (const event of events) {
      // Statistics by type
      stats.byType[event.type] = (stats.byType[event.type] || 0) + 1;

      // Statistics by thread
      if (event.threadId) {
        stats.byThread[event.threadId] = (stats.byThread[event.threadId] || 0) + 1;
      }

      // Statistics by workflow
      if (event.workflowId) {
        stats.byWorkflow[event.workflowId] = (stats.byWorkflow[event.workflowId] || 0) + 1;
      }
    }

    return stats;
  }

  /**
   * Get recent events
   * @param count Event count
   * @param filter Filter conditions
   * @returns Recent event array
   */
  async getRecentEvents(count: number, filter?: EventFilter): Promise<BaseEvent[]> {
    let events = this.eventHistory;

    // Apply filter conditions
    if (filter) {
      events = this.applyFilter(events, filter);
    }

    // Sort by timestamp descending, return the most recent count events
    return events.sort((a, b) => b.timestamp - a.timestamp).slice(0, count);
  }

  /**
   * Search events
   * @param query Search keyword
   * @param filter Filter conditions
   * @returns Matching event array
   */
  async searchEvents(query: string, filter?: EventFilter): Promise<BaseEvent[]> {
    let events = this.eventHistory;

    // Apply filter conditions
    if (filter) {
      events = this.applyFilter(events, filter);
    }

    return events.filter(event => {
      // Search event type, thread ID, workflow ID, etc.
      const searchableFields = [
        event.type,
        event.threadId,
        event.workflowId,
        `${event.type}-${event.threadId}-${event.timestamp}`,
      ];

      if ("nodeId" in event) {
        searchableFields.push((event as { nodeId?: string }).nodeId);
      }

      return searchableFields.some(
        field => field && field.toLowerCase().includes(query.toLowerCase()),
      );
    });
  }

  /**
   * Get event timeline
   * @param threadId Thread ID
   * @param workflowId Workflow ID
   * @returns Timeline event array
   */
  async getEventTimeline(threadId?: string, workflowId?: string): Promise<BaseEvent[]> {
    let events = this.eventHistory;

    // Apply filter conditions
    if (threadId) {
      events = events.filter(event => event.threadId === threadId);
    }
    if (workflowId) {
      events = events.filter(event => event.workflowId === workflowId);
    }

    // Sort by timestamp ascending to form timeline
    return events.sort((a, b) => a.timestamp - b.timestamp);
  }

  /**
   * Get event type statistics
   * @returns Event type statistics
   */
  async getEventTypeStatistics(): Promise<Record<string, number>> {
    const stats: Record<string, number> = {};

    for (const event of this.eventHistory) {
      stats[event.type] = (stats[event.type] || 0) + 1;
    }

    return stats;
  }

  /**
   * Get thread event statistics
   * @returns Thread event statistics
   */
  async getThreadEventStatistics(): Promise<Record<string, number>> {
    const stats: Record<string, number> = {};

    for (const event of this.eventHistory) {
      if (event.threadId) {
        stats[event.threadId] = (stats[event.threadId] || 0) + 1;
      }
    }

    return stats;
  }

  /**
   * Get workflow event statistics
   * @returns Workflow event statistics
   */
  async getWorkflowEventStatistics(): Promise<Record<string, number>> {
    const stats: Record<string, number> = {};

    for (const event of this.eventHistory) {
      if (event.workflowId) {
        stats[event.workflowId] = (stats[event.workflowId] || 0) + 1;
      }
    }

    return stats;
  }

  /**
   * Clear event history
   */
  async clearEventHistory(): Promise<void> {
    this.eventHistory = [];
  }

  /**
   * Get event history size
   * @returns Event count
   */
  async getEventHistorySize(): Promise<number> {
    return this.eventHistory.length;
  }

  /**
   * Get event time range
   * @returns Time range
   */
  async getEventTimeRange(): Promise<{ start: number; end: number } | null> {
    if (this.eventHistory.length === 0) {
      return null;
    }

    const timestamps = this.eventHistory.map(event => event.timestamp);
    return {
      start: Math.min(...timestamps),
      end: Math.max(...timestamps),
    };
  }
}
