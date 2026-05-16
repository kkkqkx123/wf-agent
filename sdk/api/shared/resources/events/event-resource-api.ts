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
import type { Event, EventType, Timestamp } from "@wf-agent/types";
import { DispatchEventCommand } from "../../operations/events/dispatch-event-command.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import { CommandExecutor } from "../../common/command-executor.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "EventResourceAPI" });

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
 * EventResourceAPI - Event Resource Management API
 *
 * Refactoring Notes:
 * - Removed invalid memory cache
 * - Collects event history by listening to EventRegistry
 * - Provides real-time event query and statistics functionality
 */
export class EventResourceAPI extends ReadonlyResourceAPI<Event, string, EventFilter> {
  private dependencies: APIDependencyManager;
  private executor: CommandExecutor;

  constructor(
    dependencies: APIDependencyManager,
  ) {
    super();
    this.dependencies = dependencies;
    this.executor = new CommandExecutor();
  }



  /**
   * Cleanup resources
   */
  public dispose(): void {
    // No cleanup needed - metrics are managed by EventRegistry
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
   * Get single event
   * @deprecated Event history is not stored globally. Use execution-scoped listeners instead.
   * @param id Event ID
   * @returns Always returns null as events are not persisted
   */
  protected async getResource(_id: string): Promise<Event | null> {
    logger.warn('getResource is deprecated - events are not stored globally');
    return null;
  }

  /**
   * Get all events
   * @deprecated Event history is not stored globally. Use execution-scoped listeners instead.
   * @returns Always returns empty array as events are not persisted
   */
  protected async getAllResources(): Promise<Event[]> {
    logger.warn('getAllResources is deprecated - events are not stored globally');
    return [];
  }

  /**
   * Apply filter conditions
   * @deprecated Filtering requires event history which is not available
   */
  protected override applyFilter(events: Event[], _filter: EventFilter): Event[] {
    logger.warn('applyFilter is deprecated - no event history available for filtering');
    return events;
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
    await this.executor.execute(command);
  }

  /**
   * Get event list
   * @deprecated Event history is not stored. Use execution-scoped listeners for real-time event collection.
   * @param filter Filter conditions (ignored)
   * @returns Always returns empty array
   */
  async getEvents(filter?: EventFilter): Promise<Event[]> {
    logger.warn('getEvents is deprecated - use execution-scoped listeners instead', { filter });
    return [];
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
   * Get recent events
   * @deprecated Event history is not stored. Use execution-scoped listeners for real-time event collection.
   * @param count Event count (ignored)
   * @param filter Filter conditions (ignored)
   * @returns Always returns empty array
   */
  async getRecentEvents(count: number, filter?: EventFilter): Promise<Event[]> {
    logger.warn('getRecentEvents is deprecated - use execution-scoped listeners instead', { count, filter });
    return [];
  }

  /**
   * Search events
   * @deprecated Event history is not stored. Use execution-scoped listeners for real-time event collection.
   * @param query Search keyword (ignored)
   * @param filter Filter conditions (ignored)
   * @returns Always returns empty array
   */
  async searchEvents(query: string, filter?: EventFilter): Promise<Event[]> {
    logger.warn('searchEvents is deprecated - use execution-scoped listeners instead', { query, filter });
    return [];
  }

  /**
   * Get event timeline
   * @deprecated Event history is not stored. Use execution-scoped listeners for real-time event collection.
   * @param executionId Execution ID (ignored)
   * @param workflowId Workflow ID (ignored)
   * @returns Always returns empty array
   */
  async getEventTimeline(executionId?: string, workflowId?: string): Promise<Event[]> {
    logger.warn('getEventTimeline is deprecated - use execution-scoped listeners instead', { executionId, workflowId });
    return [];
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
   * @deprecated Workflow-level tracking requires additional metrics labels
   * @returns Empty object (workflow tracking not yet implemented in metrics)
   */
  async getWorkflowEventStatistics(): Promise<Record<string, number>> {
    logger.warn('getWorkflowEventStatistics: workflow-level tracking not yet implemented');
    return {};
  }

  /**
   * Clear event history
   * @deprecated Event history is not stored. Metrics cleanup is handled by EventRegistry.
   */
  async clearEventHistory(): Promise<void> {
    logger.warn('clearEventHistory is deprecated - metrics are managed by EventRegistry');
  }

  /**
   * Get event history size
   * @deprecated Event history is not stored. Use metrics summary for total event count.
   * @returns Always returns 0
   */
  async getEventHistorySize(): Promise<number> {
    logger.warn('getEventHistorySize is deprecated - use metrics summary instead');
    return 0;
  }

  /**
   * Get event time range
   * @deprecated Event history is not stored. Use metrics for timestamp information.
   * @returns Always returns null
   */
  async getEventTimeRange(): Promise<{ start: number; end: number } | null> {
    logger.warn('getEventTimeRange is deprecated - use metrics summary instead');
    return null;
  }

  // ============================================================================
  // Agent-specific methods
  // ============================================================================

  /**
   * Get agent execution events
   * @deprecated Event history is not stored. Use execution-scoped listeners for agent events.
   * @param agentLoopId Agent Loop ID (ignored)
   * @returns Always returns empty array
   */
  async getAgentEvents(agentLoopId: string): Promise<Event[]> {
    logger.warn('getAgentEvents is deprecated - use execution-scoped listeners instead', { agentLoopId });
    return [];
  }

  /**
   * Get agent turn events
   * @deprecated Event history is not stored. Use execution-scoped listeners for agent events.
   * @param agentLoopId Agent Loop ID (ignored)
   * @returns Always returns empty array
   */
  async getAgentTurnEvents(agentLoopId: string): Promise<Event[]> {
    logger.warn('getAgentTurnEvents is deprecated - use execution-scoped listeners instead', { agentLoopId });
    return [];
  }

  /**
   * Get agent tool execution events
   * @deprecated Event history is not stored. Use execution-scoped listeners for agent events.
   * @param agentLoopId Agent Loop ID (ignored)
   * @returns Always returns empty array
   */
  async getAgentToolExecutionEvents(agentLoopId: string): Promise<Event[]> {
    logger.warn('getAgentToolExecutionEvents is deprecated - use execution-scoped listeners instead', { agentLoopId });
    return [];
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
