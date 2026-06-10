/**
 * EventResourceAPI - Event Resource Management API
 *
 * Design Notes:
 * - Provides event dispatching and metrics query functionality
 * - Event statistics are collected through EventRegistry's metrics system
 * - Events cannot be created, updated, or deleted through API
 * - Historical event queries should use execution-scoped listeners
 *
 * Note: This is now a shared API component as events are a cross-module concern
 * used by Graph, Agent, and other modules.
 */

import { ReadonlyResourceAPI } from "../generic-resource-api.js";
import type { Event, EventType, Timestamp, BaseEvent } from "@wf-agent/types";
import { DispatchEventCommand } from "../../operations/events/dispatch-event-command.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "EventResourceAPI" });

/**
 * EventResourceAPI configuration
 */
export interface EventResourceAPIConfig {
  /** Maximum number of events to keep in history (default: 1000) */
  maxHistorySize?: number;
  
  /** Enable automatic subscription to EventRegistry (default: true) */
  enableAutoSubscription?: boolean;
}

/**
 * Event filter
 */
export interface EventFilter {
  /** Event ID list */
  ids?: string[];
  /** Event type (single) */
  eventType?: EventType;
  /** Event types (multiple, OR logic) */
  eventTypes?: EventType[];
  /** Execution ID */
  executionId?: string;
  /** Workflow ID */
  workflowId?: string;
  /** Node ID */
  nodeId?: string;
  /** Agent Loop ID */
  agentLoopId?: string;
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
  /** Statistics by execution */
  byExecution: Record<string, number>;
  /** Statistics by workflow */
  byWorkflow: Record<string, number>;
}

/**
 * Execution timeline phase
 * Represents a distinct phase in the execution lifecycle
 */
export interface ExecutionTimelinePhase {
  /** Phase name (e.g., "Initialization", "Node Execution", "Checkpoint") */
  name: string;
  /** Event type that marks the start of this phase */
  startEvent: EventType;
  /** Event type that marks the end of this phase */
  endEvent: EventType;
  /** Start timestamp */
  startTime: Timestamp;
  /** End timestamp (0 if phase is still active) */
  endTime: Timestamp;
  /** Duration in milliseconds (-1 if phase is still active) */
  duration: number;
  /** Events in this phase */
  events: Event[];
}

/**
 * Execution timeline
 * Provides a structured view of an execution's lifecycle
 */
export interface ExecutionTimeline {
  /** Execution ID */
  executionId: string;
  /** Workflow ID */
  workflowId?: string;
  /** Overall execution status based on events */
  status: "running" | "completed" | "failed" | "paused" | "cancelled";
  /** Timeline start timestamp */
  startTime: Timestamp;
  /** Timeline end timestamp (0 if still running) */
  endTime: Timestamp;
  /** Total elapsed time in milliseconds */
  totalElapsed: number;
  /** Phases in chronological order */
  phases: ExecutionTimelinePhase[];
  /** All events in chronological order */
  events: Event[];
}

/**
 * Execution timeline summary
 * Condensed overview of key execution metrics
 */
export interface ExecutionTimelineSummary {
  /** Execution ID */
  executionId: string;
  /** Total execution time (ms) */
  totalDuration: number;
  /** Number of phases */
  phaseCount: number;
  /** Number of events */
  eventCount: number;
  /** Number of node executions */
  nodeExecutionCount: number;
  /** Number of checkpoint events */
  checkpointCount: number;
  /** Number of tool calls */
  toolCallCount: number;
  /** Number of errors */
  errorCount: number;
  /** Status */
  status: "running" | "completed" | "failed" | "paused" | "cancelled";
}

/**
 * EventResourceAPI - Event Resource Management API
 *
 * Refactoring Notes:
 * - Maintains in-memory event history by listening to EventRegistry
 * - Provides real-time event query and statistics functionality
 * - Uses circular buffer pattern for bounded memory usage
 */
export class EventResourceAPI extends ReadonlyResourceAPI<Event, string, EventFilter> {
  private dependencies: APIDependencyManager;
  
  // Event storage
  private eventHistory: Event[] = [];
  private maxHistorySize: number;
  private subscriptions: Array<() => void> = [];

  constructor(
    dependencies: APIDependencyManager,
    config?: EventResourceAPIConfig
  ) {
    super();
    this.dependencies = dependencies;
    this.maxHistorySize = config?.maxHistorySize ?? 1000;
    
    // Auto-subscribe if enabled
    if (config?.enableAutoSubscription !== false) {
      this.setupEventSubscription();
    }
  }

  /**
   * Setup automatic event subscription
   * Listens to all events and adds them to history
   */
  private setupEventSubscription(): void {
    const eventManager = this.dependencies.getEventManager();
    
    // Subscribe to all events using global listener
    const unsubscribe = eventManager.onGlobal((event: BaseEvent) => {
      // Cast to Event since all emitted events should be full Event types
      this.addToHistory(event as Event);
    });
    
    this.subscriptions.push(unsubscribe);
    
    logger.debug('EventResourceAPI subscribed to global event stream', {
      maxHistorySize: this.maxHistorySize,
    });
  }

  /**
   * Add event to history with size management
   */
  private addToHistory(event: Event): void {
    this.eventHistory.push(event);
    
    // Enforce size limit using circular buffer pattern
    if (this.eventHistory.length > this.maxHistorySize) {
      // Remove oldest events (keep most recent)
      this.eventHistory = this.eventHistory.slice(-this.maxHistorySize);
    }
  }



  /**
   * Cleanup resources
   */
  public dispose(): void {
    // Unsubscribe from all event sources
    for (const unsubscribe of this.subscriptions) {
      unsubscribe();
    }
    this.subscriptions = [];
    
    // Clear event history
    this.eventHistory = [];
    
    logger.debug('EventResourceAPI disposed');
  }

  // ============================================================================
  // Diagnostic APIs - Monitor and debug event listeners
  // ============================================================================

  /**
   * Get statistics about execution-scoped listeners
   * Useful for debugging listener lifecycle and detecting memory leaks
   * 
   * @returns Map of execution ID to active listener count
   * 
   * @example
   * ```typescript
   * const stats = api.getExecutionListenerStats();
   * for (const [executionId, count] of stats) {
   *   console.log(`Execution ${executionId}: ${count} listeners`);
   * }
   * ```
   */
  getExecutionListenerStats(): Map<string, number> {
    return this.dependencies.getEventManager().getExecutionListenerStats();
  }


  /**
   * Get summary of event system health
   * Combines multiple diagnostic metrics into a single overview
   * 
   * @returns Event system health summary
   * 
   * @example
   * ```typescript
   * const health = api.getEventSystemHealth();
   * console.log(`Active executions with listeners: ${health.activeExecutionCount}`);
   * console.log(`Total events in metrics: ${health.totalMetricsEvents}`);
   * ```
   */
  getEventSystemHealth(): {
    activeExecutionCount: number;
    totalMetricsEvents: number;
    executionStats: Map<string, number>;
  } {
    const executionStats = this.getExecutionListenerStats();
    const metricsCollector = this.dependencies.getEventManager().getMetricsCollector();
    const summary = metricsCollector.generateSummary();

    return {
      activeExecutionCount: executionStats.size,
      totalMetricsEvents: summary.totalEvents,
      executionStats,
    };
  }

  // ============================================================================
  // Implement abstract methods
  // ============================================================================

  /**
   * Get single event by ID
   * @param id Event ID
   * @returns Event or null if not found
   */
  protected async getResource(id: string): Promise<Event | null> {
    const event = this.eventHistory.find(e => e.id === id);
    return event || null;
  }

  /**
   * Get all events in history
   * @returns Copy of event history array
   */
  protected async getAllResources(): Promise<Event[]> {
    // Return copy to prevent external mutation
    return [...this.eventHistory];
  }

  /**
   * Apply filter conditions to events
   * Supports filtering by type, execution, workflow, node, time range, etc.
   */
  protected override applyFilter(events: Event[], filter: EventFilter): Event[] {
    return events.filter(event => {
      // Filter by event type (single)
      if (filter.eventType && event.type !== filter.eventType) {
        return false;
      }
      
      // Filter by event types (multiple, OR logic)
      if (filter.eventTypes && !filter.eventTypes.includes(event.type)) {
        return false;
      }
      
      // Filter by execution ID
      if (filter.executionId && event.executionId !== filter.executionId) {
        return false;
      }
      
      // Filter by workflow ID
      if (filter.workflowId && event.workflowId !== filter.workflowId) {
        return false;
      }
      
      // Filter by node ID (type-safe check)
      if (filter.nodeId) {
        if (!('nodeId' in event) || event.nodeId !== filter.nodeId) {
          return false;
        }
      }
      
      // Filter by agent loop ID (type-safe check)
      if (filter.agentLoopId) {
        if (!('agentLoopId' in event) || event.agentLoopId !== filter.agentLoopId) {
          return false;
        }
      }
      
      // Filter by timestamp range
      if (filter.timestampRange) {
        if (filter.timestampRange.start && event.timestamp < filter.timestampRange.start) {
          return false;
        }
        if (filter.timestampRange.end && event.timestamp > filter.timestampRange.end) {
          return false;
        }
      }
      
      // Filter by event IDs list
      if (filter.ids && !filter.ids.includes(event.id)) {
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
  async dispatch(event: Event): Promise<void> {
    const command = new DispatchEventCommand({ event }, this.dependencies);
    const validation = command.validate();
    if (!validation.valid) {
      throw new Error(
        `Failed to dispatch event: ${validation.errors.join(", ")}`,
      );
    }
    const result = await command.execute();
    if (result.result.isErr()) {
      throw result.result.error;
    }
  }

  /**
   * Get event list with optional filtering
   * @param filter Filter conditions
   * @returns Filtered events from history
   */
  async getEvents(filter?: EventFilter): Promise<Event[]> {
    let events = [...this.eventHistory];
    
    if (filter) {
      events = this.applyFilter(events, filter);
    }
    
    return events;
  }

  /**
   * Get most recent events
   * @param count Number of events to retrieve
   * @param filter Optional filter applied before limiting
   * @returns Most recent events (sorted by timestamp descending)
   */
  async getRecentEvents(count: number, filter?: EventFilter): Promise<Event[]> {
    let events = [...this.eventHistory];
    
    // Apply filter first
    if (filter) {
      events = this.applyFilter(events, filter);
    }
    
    // Sort by timestamp (newest first) and take top N
    events.sort((a, b) => b.timestamp - a.timestamp);
    return events.slice(0, count);
  }

  /**
   * Search events by keyword in relevant fields
   * @param query Search keyword
   * @param filter Additional filter conditions
   * @returns Matching events
   */
  async searchEvents(query: string, filter?: EventFilter): Promise<Event[]> {
    if (!query || query.trim().length === 0) {
      return [];
    }
    
    const lowerQuery = query.toLowerCase();
    let events = [...this.eventHistory];
    
    // Apply standard filters first
    if (filter) {
      events = this.applyFilter(events, filter);
    }
    
    // Search in searchable fields
    return events.filter(event => {
      const searchableFields: string[] = [];
      
      // Event type
      searchableFields.push(event.type.toLowerCase());
      
      // Execution ID
      if (event.executionId) {
        searchableFields.push(event.executionId.toLowerCase());
      }
      
      // Workflow ID
      if (event.workflowId) {
        searchableFields.push(event.workflowId.toLowerCase());
      }
      
      // Node ID (if present)
      if ('nodeId' in event && event.nodeId) {
        searchableFields.push((event as { nodeId: string }).nodeId.toLowerCase());
      }
      
      // Check if query matches any field
      return searchableFields.some(field => field.includes(lowerQuery));
    });
  }

  /**
   * Get event timeline for specific execution or workflow
   * @param executionId Optional execution ID filter
   * @param workflowId Optional workflow ID filter
   * @returns Events sorted chronologically
   */
  async getEventTimeline(executionId?: string, workflowId?: string): Promise<Event[]> {
    let events = [...this.eventHistory];
    
    // Filter by execution or workflow
    if (executionId) {
      events = events.filter(e => e.executionId === executionId);
    } else if (workflowId) {
      events = events.filter(e => e.workflowId === workflowId);
    }
    
    // Sort by timestamp (chronological order)
    events.sort((a, b) => a.timestamp - b.timestamp);
    
    return events;
  }

  /**
   * Phase definitions for building execution timelines
   * Maps event type pairs to phase names
   */
  private static readonly PHASE_DEFINITIONS: Array<{
    name: string;
    start: EventType;
    end: EventType;
  }> = [
    { name: "Execution", start: "WORKFLOW_EXECUTION_STARTED", end: "WORKFLOW_EXECUTION_COMPLETED" },
    { name: "Node Execution", start: "NODE_STARTED", end: "NODE_COMPLETED" },
    { name: "Fork", start: "FORK_STARTED", end: "FORK_COMPLETED" },
    { name: "Join", start: "WORKFLOW_EXECUTION_JOIN_STARTED", end: "WORKFLOW_EXECUTION_JOIN_COMPLETED" },
    { name: "Tool Call", start: "TOOL_CALL_STARTED", end: "TOOL_CALL_COMPLETED" },
    { name: "Agent Turn", start: "AGENT_TURN_STARTED", end: "AGENT_TURN_COMPLETED" },
    { name: "Agent Iteration", start: "AGENT_ITERATION_STARTED", end: "AGENT_ITERATION_COMPLETED" },
  ];

  /**
   * Get structured execution timeline with phases
   * Groups events into lifecycle phases with timing analysis
   * @param executionId Execution ID
   * @returns Structured execution timeline
   */
  async getExecutionTimeline(executionId: string): Promise<ExecutionTimeline | null> {
    const events = await this.getEventTimeline(executionId);

    if (events.length === 0) {
      return null;
    }

    // Determine overall status from events
    const status = this.determineTimelineStatus(events);
    const startTime = events[0]!.timestamp;
    const lastEvent = events[events.length - 1]!;
    const endTime = lastEvent.timestamp;

    // Build phases using start/end event pairs
    const phases = this.buildPhases(events);

    return {
      executionId,
      workflowId: events.find(e => e.workflowId)?.workflowId,
      status,
      startTime,
      endTime,
      totalElapsed: endTime - startTime,
      phases,
      events,
    };
  }

  /**
   * Get condensed execution timeline summary
   * @param executionId Execution ID
   * @returns Timeline summary metrics
   */
  async getExecutionTimelineSummary(executionId: string): Promise<ExecutionTimelineSummary | null> {
    const timeline = await this.getExecutionTimeline(executionId);

    if (!timeline) {
      return null;
    }

    // Count events by category
    let nodeExecutionCount = 0;
    let checkpointCount = 0;
    let toolCallCount = 0;
    let errorCount = 0;

    for (const event of timeline.events) {
      if (event.type === "NODE_STARTED" || event.type === "NODE_COMPLETED") {
        nodeExecutionCount++;
      } else if (event.type.startsWith("CHECKPOINT_")) {
        checkpointCount++;
      } else if (event.type.startsWith("TOOL_CALL_")) {
        toolCallCount++;
      } else if (event.type === "ERROR" || event.type.endsWith("_FAILED")) {
        errorCount++;
      }
    }

    return {
      executionId,
      totalDuration: timeline.totalElapsed,
      phaseCount: timeline.phases.length,
      eventCount: timeline.events.length,
      nodeExecutionCount,
      checkpointCount,
      toolCallCount,
      errorCount,
      status: timeline.status,
    };
  }

  /**
   * Determine overall execution status from events
   */
  private determineTimelineStatus(
    events: Event[],
  ): "running" | "completed" | "failed" | "paused" | "cancelled" {
    for (let i = events.length - 1; i >= 0; i--) {
      const event = events[i]!;
      if (event.type === "WORKFLOW_EXECUTION_COMPLETED") return "completed";
      if (event.type === "WORKFLOW_EXECUTION_FAILED") return "failed";
      if (event.type === "WORKFLOW_EXECUTION_PAUSED") return "paused";
      if (event.type === "WORKFLOW_EXECUTION_CANCELLED") return "cancelled";
    }
    return "running";
  }

  /**
   * Build phases from events using phase definitions
   */
  private buildPhases(events: Event[]): ExecutionTimelinePhase[] {
    const phases: ExecutionTimelinePhase[] = [];
    const eventMap = new Map<EventType, Event[]>();

    // Group events by type
    for (const event of events) {
      const list = eventMap.get(event.type);
      if (list) {
        list.push(event);
      } else {
        eventMap.set(event.type, [event]);
      }
    }

    // Build phases from matched start/end pairs
    for (const def of EventResourceAPI.PHASE_DEFINITIONS) {
      const startEvents = eventMap.get(def.start);
      const endEvents = eventMap.get(def.end);

      if (!startEvents || startEvents.length === 0) {
        continue;
      }

      // Pair up start and end events (stack-based matching)
      const usedStarts = new Set<number>();
      const usedEnds = new Set<number>();

      // Match starts to ends in order
      for (let si = 0; si < startEvents.length; si++) {
        if (!endEvents || endEvents.length === 0) {
          // No end events, all starts are open phases
          usedStarts.add(si);
          const startEvent = startEvents[si]!;
          phases.push({
            name: def.name,
            startEvent: def.start,
            endEvent: def.end,
            startTime: startEvent.timestamp,
            endTime: 0,
            duration: -1,
            events: events.filter(e => e.timestamp >= startEvent.timestamp),
          });
          continue;
        }
        for (let ei = 0; ei < endEvents.length; ei++) {
          if (usedStarts.has(si) || usedEnds.has(ei)) continue;
          const startEvent = startEvents[si]!;
          const endEvent = endEvents[ei]!;
          if (endEvent.timestamp >= startEvent.timestamp) {
            usedStarts.add(si);
            usedEnds.add(ei);

            const phaseEvents = events.filter(
              e => e.timestamp >= startEvent.timestamp && e.timestamp <= endEvent.timestamp,
            );

            phases.push({
              name: def.name,
              startEvent: def.start,
              endEvent: def.end,
              startTime: startEvent.timestamp,
              endTime: endEvent.timestamp,
              duration: endEvent.timestamp - startEvent.timestamp,
              events: phaseEvents,
            });
            break;
          }
        }
      }

      // Add unmatched starts as open phases
      for (let si = 0; si < startEvents.length; si++) {
        if (usedStarts.has(si)) continue;
        const startEvent = startEvents[si]!;
        phases.push({
          name: def.name,
          startEvent: def.start,
          endEvent: def.end,
          startTime: startEvent.timestamp,
          endTime: 0,
          duration: -1,
          events: events.filter(e => e.timestamp >= startEvent.timestamp),
        });
      }
    }

    // Sort phases by start time
    phases.sort((a, b) => a.startTime - b.startTime);

    return phases;
  }

  /**
   * Get event statistics from metrics collector
   * @param filter Filter conditions (partially supported via metrics labels)
   * @returns Statistics aggregated from metrics system
   */
  async getEventStats(_filter?: EventFilter): Promise<EventStats> {
    const metricsCollector = this.dependencies.getEventManager().getMetricsCollector();
    const summary = metricsCollector.generateSummary();

    const stats: EventStats = {
      total: summary.totalEvents,
      byType: {},
      byExecution: {},
      byWorkflow: {},
    };

    // Convert metrics to stats format
    for (const [eventType, stat] of summary.byEventType.entries()) {
      stats.byType[eventType] = stat.count;
      
      // Aggregate by execution
      for (const [executionId, count] of stat.byExecution.entries()) {
        stats.byExecution[executionId] = (stats.byExecution[executionId] || 0) + count;
      }
    }

    // Note: workflow-level aggregation would require additional label support
    // For now, byWorkflow remains empty as it's not tracked in current metrics implementation

    return stats;
  }


  /**
   * Get event type statistics from metrics collector
   * @returns Event type statistics aggregated from metrics system
   */
  async getEventTypeStatistics(): Promise<Record<string, number>> {
    const metricsCollector = this.dependencies.getEventManager().getMetricsCollector();
    const summary = metricsCollector.generateSummary();

    const stats: Record<string, number> = {};
    for (const [eventType, stat] of summary.byEventType.entries()) {
      stats[eventType] = stat.count;
    }

    return stats;
  }

  /**
   * Get workflow execution event statistics from metrics collector
   * @returns Workflow execution event statistics aggregated from metrics system
   */
  async getWorkflowExecutionEventStatistics(): Promise<Record<string, number>> {
    const metricsCollector = this.dependencies.getEventManager().getMetricsCollector();
    const summary = metricsCollector.generateSummary();

    const stats: Record<string, number> = {};
    
    // Aggregate counts by execution ID from all event types
    for (const stat of summary.byEventType.values()) {
      for (const [executionId, count] of stat.byExecution.entries()) {
        stats[executionId] = (stats[executionId] || 0) + count;
      }
    }

    return stats;
  }

  /**
   * Get workflow event statistics
   * Note: Workflow-level tracking requires additional metrics labels
   * @returns Empty object (workflow tracking not yet implemented in metrics)
   */
  async getWorkflowEventStatistics(): Promise<Record<string, number>> {
    logger.warn('getWorkflowEventStatistics: workflow-level tracking not yet implemented');
    return {};
  }

  /**
   * Clear all events from history
   * Useful for testing or memory management
   */
  async clearEventHistory(): Promise<void> {
    const clearedCount = this.eventHistory.length;
    this.eventHistory = [];
    
    logger.info('Event history cleared', { clearedCount });
  }

  /**
   * Get current event history size
   * @returns Number of events in history
   */
  async getEventHistorySize(): Promise<number> {
    return this.eventHistory.length;
  }

  /**
   * Get time range of events in history
   * @returns Start and end timestamps, or null if empty
   */
  async getEventTimeRange(): Promise<{ start: number; end: number } | null> {
    if (this.eventHistory.length === 0) {
      return null;
    }
    
    const timestamps = this.eventHistory.map(e => e.timestamp);
    return {
      start: Math.min(...timestamps),
      end: Math.max(...timestamps),
    };
  }

  // ============================================================================
  // Agent-specific methods
  // ============================================================================

  /**
   * Get agent execution events
   * @param agentLoopId Agent Loop ID
   * @returns Events related to specific agent loop
   */
  async getAgentEvents(agentLoopId: string): Promise<Event[]> {
    return this.eventHistory.filter(event => 
      'agentLoopId' in event && event.agentLoopId === agentLoopId
    );
  }

  /**
   * Get agent turn events
   * @param agentLoopId Agent Loop ID
   * @returns Turn-related events for agent
   */
  async getAgentTurnEvents(agentLoopId: string): Promise<Event[]> {
    return this.eventHistory.filter(event => 
      'agentLoopId' in event && 
      event.agentLoopId === agentLoopId &&
      (event.type === 'AGENT_TURN_STARTED' || event.type === 'AGENT_TURN_COMPLETED')
    );
  }

  /**
   * Get agent tool execution events
   * @param agentLoopId Agent Loop ID
   * @returns Tool execution events for agent
   */
  async getAgentToolExecutionEvents(agentLoopId: string): Promise<Event[]> {
    return this.eventHistory.filter(event => 
      'agentLoopId' in event && 
      event.agentLoopId === agentLoopId &&
      (event.type === 'AGENT_TOOL_EXECUTION_STARTED' || 
       event.type === 'AGENT_TOOL_EXECUTION_COMPLETED')
    );
  }

  /**
   * Get agent statistics by loop from metrics collector
   * @returns Statistics grouped by agent loop ID (requires metrics with agent_loop_id label)
   */
  async getAgentLoopStatistics(): Promise<Record<string, number>> {
    // Note: Agent loop statistics would require metrics with agent_loop_id label
    // Current implementation returns empty as this label is not tracked
    logger.warn('getAgentLoopStatistics: agent loop tracking requires metrics with agent_loop_id label');
    return {};
  }
}
