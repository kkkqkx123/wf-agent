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
import type { Event, EventType } from "@wf-agent/types";
import { isNodeEvent, isAgentEvent, hasAgentLoopId } from "@wf-agent/types";
import type { Timestamp } from "@wf-agent/types";
import { DispatchEventCommand } from "../../operations/events/dispatch-event-command.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import { CommandExecutor } from "../../common/command-executor.js";

/**
 * All event types list
 * Used to listen to all events
 */
const ALL_EVENT_TYPES: EventType[] = [
  // Workflow Execution events
  "WORKFLOW_EXECUTION_STARTED",
  "WORKFLOW_EXECUTION_COMPLETED",
  "WORKFLOW_EXECUTION_FAILED",
  "WORKFLOW_EXECUTION_PAUSED",
  "WORKFLOW_EXECUTION_RESUMED",
  "WORKFLOW_EXECUTION_CANCELLED",
  "WORKFLOW_EXECUTION_STATE_CHANGED",
  "WORKFLOW_EXECUTION_FORK_STARTED",
  "WORKFLOW_EXECUTION_FORK_COMPLETED",
  "WORKFLOW_EXECUTION_JOIN_STARTED",
  "WORKFLOW_EXECUTION_JOIN_CONDITION_MET",
  "WORKFLOW_EXECUTION_COPY_STARTED",
  "WORKFLOW_EXECUTION_COPY_COMPLETED",
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
  // Tool approval events (specialized)
  "TOOL_APPROVAL_REQUESTED",
  "TOOL_APPROVAL_RESPONDED",
  "TOOL_APPROVAL_FAILED",
  // Follow-up question events (specialized)
  "FOLLOWUP_QUESTION_REQUESTED",
  "FOLLOWUP_QUESTION_RESPONDED",
  "FOLLOWUP_QUESTION_FAILED",
  // HumanRelay events
  "HUMAN_RELAY_REQUESTED",
  "HUMAN_RELAY_RESPONDED",
  "HUMAN_RELAY_FAILED",
  // LLM stream events
  "LLM_STREAM_ABORTED",
  "LLM_STREAM_ERROR",
  // Agent events
  "AGENT_HOOK_TRIGGERED",
  // Skill events
  "SKILL_LOAD_STARTED",
  "SKILL_LOAD_COMPLETED",
  "SKILL_LOAD_FAILED",
  // Agent lifecycle events
  "AGENT_STARTED",
  "AGENT_COMPLETED",
  "AGENT_TURN_STARTED",
  "AGENT_TURN_COMPLETED",
  "AGENT_MESSAGE_STARTED",
  "AGENT_MESSAGE_COMPLETED",
  "AGENT_TOOL_EXECUTION_STARTED",
  "AGENT_TOOL_EXECUTION_COMPLETED",
  "AGENT_ITERATION_COMPLETED",
  // Agent interruption events
  "AGENT_PAUSED",
  "AGENT_CANCELLED",
];

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
 * EventResourceAPI configuration options
 */
export interface EventResourceAPIConfig {
  /** Maximum number of events to keep in history (default: 1000) */
  maxHistorySize?: number;
  /** Retention time in milliseconds for automatic cleanup (optional) */
  retentionTimeMs?: number;
  /** Enable persistence for critical events (future feature flag) */
  persistCriticalEvents?: boolean;
  /** List of event types to consider as critical (if persistCriticalEvents is true) */
  criticalEventTypes?: EventType[];
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
  private eventHistory: Event[] = [];
  private dependencies: APIDependencyManager;
  private executor: CommandExecutor;
  private unsubscribe?: () => void;
  private config: Required<Omit<EventResourceAPIConfig, 'retentionTimeMs' | 'persistCriticalEvents' | 'criticalEventTypes'>> & Pick<EventResourceAPIConfig, 'retentionTimeMs' | 'persistCriticalEvents' | 'criticalEventTypes'>;
  private cleanupTimer?: NodeJS.Timeout;

  constructor(
    dependencies: APIDependencyManager,
    configOrMaxHistorySize?: EventResourceAPIConfig | number,
  ) {
    super();
    this.dependencies = dependencies;
    this.executor = new CommandExecutor();

    // Support both old API (number) and new API (config object)
    if (typeof configOrMaxHistorySize === 'number') {
      // Legacy API: just maxHistorySize
      this.config = {
        maxHistorySize: configOrMaxHistorySize,
        retentionTimeMs: undefined,
        persistCriticalEvents: false,
        criticalEventTypes: undefined,
      };
    } else {
      // New API: config object
      this.config = {
        maxHistorySize: configOrMaxHistorySize?.maxHistorySize ?? 1000,
        retentionTimeMs: configOrMaxHistorySize?.retentionTimeMs,
        persistCriticalEvents: configOrMaxHistorySize?.persistCriticalEvents ?? false,
        criticalEventTypes: configOrMaxHistorySize?.criticalEventTypes,
      };
    }

    // Setup event listeners
    this.setupEventListeners();
    
    // Start retention cleanup timer if configured
    if (this.config.retentionTimeMs) {
      this.startRetentionCleanup();
    }
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
  private addEventToHistory(event: Event): void {
    this.eventHistory.push(event);

    // Auto-trim history based on size limit
    if (this.eventHistory.length > this.config.maxHistorySize) {
      this.eventHistory = this.eventHistory.slice(-this.config.maxHistorySize);
    }
  }

  /**
   * Start retention-based cleanup timer
   */
  private startRetentionCleanup(): void {
    if (!this.config.retentionTimeMs) return;

    const interval = Math.min(this.config.retentionTimeMs, 60 * 1000); // Check at most every minute
    
    this.cleanupTimer = setInterval(() => {
      this.cleanupExpiredEvents();
    }, interval);

    // Ensure timer doesn't prevent process exit
    if (this.cleanupTimer.unref) {
      this.cleanupTimer.unref();
    }
  }

  /**
   * Cleanup events older than retention time
   */
  private cleanupExpiredEvents(): void {
    if (!this.config.retentionTimeMs) return;

    const now = Date.now();
    const cutoffTime = now - this.config.retentionTimeMs;
    
    const originalLength = this.eventHistory.length;
    this.eventHistory = this.eventHistory.filter(event => event.timestamp >= cutoffTime);
    
    const removedCount = originalLength - this.eventHistory.length;
    if (removedCount > 0) {
      // Log cleanup in development/debug mode
      if (process.env['NODE_ENV'] !== 'production') {
        console.debug(`Cleaned up ${removedCount} expired events`);
      }
    }
  }

  /**
   * Cleanup resources
   */
  public dispose(): void {
    // Clear retention cleanup timer
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = undefined;
    }
    
    // Unsubscribe from event listeners
    if (this.unsubscribe) {
      this.unsubscribe();
      this.unsubscribe = undefined;
    }
    
    // Clear event history
    this.eventHistory = [];
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
   * Get detailed information about all active listeners
   * Provides comprehensive view for debugging and monitoring
   * 
   * @returns Array of listener information including metrics
   * 
   * @example
   * ```typescript
   * const listeners = api.getAllListenerInfo();
   * 
   * // Find execution-scoped listeners
   * const scopedListeners = listeners.filter(l => l.executionId !== undefined);
   * 
   * // Find slow listeners
   * const slowListeners = listeners.filter(l => 
   *   l.metrics && l.metrics.slowExecutionCount > 0
   * );
   * ```
   */
  getAllListenerInfo(): Array<{
    id: string;
    eventType: string;
    executionId?: string;
    priority: number;
    registeredAt: number;
    metrics?: {
      totalExecutions: number;
      averageDuration: number;
      failureCount: number;
      slowExecutionCount: number;
    };
  }> {
    const rawInfo = this.dependencies.getEventManager().getAllListenerInfo();
    
    return rawInfo.map(info => ({
      ...info,
      metrics: info.metrics ? {
        totalExecutions: info.metrics.totalExecutions,
        averageDuration: info.metrics.averageDuration,
        failureCount: info.metrics.failureCount,
        slowExecutionCount: info.metrics.slowExecutionCount,
      } : undefined,
    }));
  }

  /**
   * Get listener counts by event type
   * Shows distribution of global vs execution-scoped listeners
   * 
   * @returns Map of event type to listener counts
   * 
   * @example
   * ```typescript
   * const counts = api.getListenerCountByEventType();
   * const nodeCompleted = counts.get('NODE_COMPLETED');
   * console.log(`NODE_COMPLETED: ${nodeCompleted?.total} total, ` +
   *             `${nodeCompleted?.global} global, ` +
   *             `${nodeCompleted?.executionScoped} execution-scoped`);
   * ```
   */
  getListenerCountByEventType(): Map<string, {
    total: number;
    executionScoped: number;
    global: number;
  }> {
    return this.dependencies.getEventManager().getListenerCountByEventType();
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
   * console.log(`Total listeners: ${health.totalListeners}`);
   * console.log(`Active executions with listeners: ${health.activeExecutionCount}`);
   * console.log(`Events in history: ${health.historySize}`);
   * ```
   */
  getEventSystemHealth(): {
    totalListeners: number;
    activeExecutionCount: number;
    historySize: number;
    listenerDistribution: Map<string, { total: number; executionScoped: number; global: number }>;
    executionStats: Map<string, number>;
  } {
    const listenerDistribution = this.getListenerCountByEventType();
    const executionStats = this.getExecutionListenerStats();
    
    let totalListeners = 0;
    for (const [, counts] of listenerDistribution) {
      totalListeners += counts.total;
    }

    return {
      totalListeners,
      activeExecutionCount: executionStats.size,
      historySize: this.eventHistory.length,
      listenerDistribution,
      executionStats,
    };
  }

  // ============================================================================
  // Implement abstract methods
  // ============================================================================

  /**
   * Get single event
   * @param id Event ID
   * @returns Event object, or null if not found
   */
  protected async getResource(id: string): Promise<Event | null> {
    return this.eventHistory.find(event => event.id === id) || null;
  }

  /**
   * Get all events
   * @returns Event array
   */
  protected async getAllResources(): Promise<Event[]> {
    return [...this.eventHistory];
  }

  /**
   * Apply filter conditions
   */
  protected override applyFilter(events: Event[], filter: EventFilter): Event[] {
    return events.filter(event => {
      // Support single event type
      if (filter.eventType && event.type !== filter.eventType) {
        return false;
      }
      
      // Support multiple event types (OR logic)
      if (filter.eventTypes && !filter.eventTypes.includes(event.type)) {
        return false;
      }
      
      if (filter.executionId && event.executionId !== filter.executionId) {
        return false;
      }
      if (filter.workflowId && event.workflowId !== filter.workflowId) {
        return false;
      }
      
      // Use type guard for safe nodeId access
      if (filter.nodeId) {
        if (!isNodeEvent(event) || event.nodeId !== filter.nodeId) {
          return false;
        }
      }
      
      // Use type guard for safe agentLoopId access
      if (filter.agentLoopId) {
        if (!hasAgentLoopId(event) || event.agentLoopId !== filter.agentLoopId) {
          return false;
        }
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
  async dispatch(event: Event): Promise<void> {
    const command = new DispatchEventCommand({ event }, this.dependencies);
    await this.executor.execute(command);
  }

  /**
   * Get event list
   * @param filter Filter conditions
   * @returns Event array
   */
  async getEvents(filter?: EventFilter): Promise<Event[]> {
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
      byExecution: {},
      byWorkflow: {},
    };

    for (const event of events) {
      // Statistics by type
      stats.byType[event.type] = (stats.byType[event.type] || 0) + 1;

      // Statistics by execution
      if (event.executionId) {
        stats.byExecution[event.executionId] = (stats.byExecution[event.executionId] || 0) + 1;
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
  async getRecentEvents(count: number, filter?: EventFilter): Promise<Event[]> {
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
  async searchEvents(query: string, filter?: EventFilter): Promise<Event[]> {
    let events = this.eventHistory;

    // Apply filter conditions
    if (filter) {
      events = this.applyFilter(events, filter);
    }

    return events.filter(event => {
      // Search event type, execution ID, workflow ID, etc.
      const searchableFields: Array<string | undefined> = [
        event.type,
        event.executionId,
        event.workflowId,
        event.id,
      ];

      // Safely add nodeId if present
      if (isNodeEvent(event)) {
        searchableFields.push(event.nodeId);
      }

      return searchableFields.some(
        field => field && field.toLowerCase().includes(query.toLowerCase()),
      );
    });
  }

  /**
   * Get event timeline
   * @param executionId Execution ID
   * @param workflowId Workflow ID
   * @returns Timeline event array
   */
  async getEventTimeline(executionId?: string, workflowId?: string): Promise<Event[]> {
    let events = this.eventHistory;

    // Apply filter conditions
    if (executionId) {
      events = events.filter(event => event.executionId === executionId);
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
   * Get workflow execution event statistics
   * @returns Workflow execution event statistics
   */
  async getWorkflowExecutionEventStatistics(): Promise<Record<string, number>> {
    const stats: Record<string, number> = {};

    for (const event of this.eventHistory) {
      if (event.executionId) {
        stats[event.executionId] = (stats[event.executionId] || 0) + 1;
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

  // ============================================================================
  // Agent-specific methods
  // ============================================================================

  /**
   * Get agent execution events
   * @param agentLoopId Agent Loop ID
   * @returns Agent events for the specified loop
   */
  async getAgentEvents(agentLoopId: string): Promise<Event[]> {
    return this.eventHistory.filter(
      event => isAgentEvent(event) && hasAgentLoopId(event) && event.agentLoopId === agentLoopId,
    );
  }

  /**
   * Get agent turn events
   * @param agentLoopId Agent Loop ID
   * @returns Turn start/completion events
   */
  async getAgentTurnEvents(agentLoopId: string): Promise<Event[]> {
    return this.eventHistory.filter(event => {
      if (!isAgentEvent(event) || !hasAgentLoopId(event)) return false;
      if (event.agentLoopId !== agentLoopId) return false;
      return event.type === 'AGENT_TURN_STARTED' || event.type === 'AGENT_TURN_COMPLETED';
    });
  }

  /**
   * Get agent tool execution events
   * @param agentLoopId Agent Loop ID
   * @returns Tool execution start/completion events
   */
  async getAgentToolExecutionEvents(agentLoopId: string): Promise<Event[]> {
    return this.eventHistory.filter(event => {
      if (!isAgentEvent(event) || !hasAgentLoopId(event)) return false;
      if (event.agentLoopId !== agentLoopId) return false;
      return (
        event.type === 'AGENT_TOOL_EXECUTION_STARTED' ||
        event.type === 'AGENT_TOOL_EXECUTION_COMPLETED'
      );
    });
  }

  /**
   * Get agent statistics by loop
   * @returns Statistics grouped by agent loop ID
   */
  async getAgentLoopStatistics(): Promise<Record<string, number>> {
    const stats: Record<string, number> = {};

    for (const event of this.eventHistory) {
      if (isAgentEvent(event) && hasAgentLoopId(event)) {
        stats[event.agentLoopId] = (stats[event.agentLoopId] || 0) + 1;
      }
    }

    return stats;
  }
}
