/**
 * Event Manager - Comprehensive execution event handling and monitoring
 */

import { EventEmitter } from 'node:events';

/**
 * Execution event types
 */
export enum ExecutionEventType {
  EXECUTION_START = 'execution:start',
  EXECUTION_PROGRESS = 'execution:progress',
  EXECUTION_COMPLETED = 'execution:completed',
  EXECUTION_FAILED = 'execution:failed',
  EXECUTION_CANCELLED = 'execution:cancelled',

  NODE_START = 'node:start',
  NODE_PROGRESS = 'node:progress',
  NODE_COMPLETED = 'node:completed',
  NODE_FAILED = 'node:failed',
  NODE_SKIPPED = 'node:skipped',

  EDGE_TRAVERSED = 'edge:traversed',
  EVENT_EMITTED = 'event:emitted',
}

/**
 * Execution event payload
 */
export interface ExecutionEventPayload {
  executionId: string;
  workflowId: string;
  timestamp: number;
  type: ExecutionEventType;
  data?: Record<string, unknown>;
  error?: Error;
  nodeId?: string;
}

/**
 * Event subscription handler
 */
export type EventHandler = (payload: ExecutionEventPayload) => void;

/**
 * Event unsubscribe function
 */
export type Unsubscribe = () => void;

/**
 * Event filter for history queries
 */
export interface EventFilter {
  executionId?: string;
  workflowId?: string;
  type?: ExecutionEventType | ExecutionEventType[];
  startTime?: number;
  endTime?: number;
}

/**
 * Event Manager implementation
 */
export class EventManager {
  private emitter: EventEmitter;
  private eventHistory: ExecutionEventPayload[] = [];
  private maxHistorySize: number = 10000;

  constructor() {
    this.emitter = new EventEmitter();
    this.emitter.setMaxListeners(100); // Increase max listeners for multiple subscribers
  }

  /**
   * Emit an execution event
   */
  emit(eventType: ExecutionEventType, payload: ExecutionEventPayload): void {
    // Record event in history
    this.addToHistory(payload);

    // Emit typed event
    this.emitter.emit(eventType, payload);

    // Emit wildcard event for all subscribers
    const wildcardPayload = Object.assign({}, payload, { eventType });
    this.emitter.emit('*', wildcardPayload);
  }

  /**
   * Subscribe to specific event(s)
   */
  subscribe(
    type: ExecutionEventType | ExecutionEventType[],
    handler: EventHandler
  ): Unsubscribe {
    const types = Array.isArray(type) ? type : [type];

    const listeners = types.map(eventType => {
      const listener = (payload: ExecutionEventPayload) => {
        try {
          handler(payload);
        } catch (error) {
          this.emitter.emit('error', error);
        }
      };

      this.emitter.on(eventType, listener);
      return { type: eventType, listener };
    });

    return () => {
      listeners.forEach(({ type: eventType, listener }) => {
        this.emitter.off(eventType, listener);
      });
    };
  }

  /**
   * Subscribe to event once (auto-unsubscribe after first emit)
   */
  subscribeOnce(type: ExecutionEventType, handler: EventHandler): void {
    const listener = (payload: ExecutionEventPayload) => {
      try {
        handler(payload);
      } catch (error) {
        this.emitter.emit('error', error);
      }
    };

    this.emitter.once(type, listener);
  }

  /**
   * Subscribe to all events
   */
  subscribeAll(handler: EventHandler): Unsubscribe {
    const listener = (eventPayload: any) => {
      try {
        handler(eventPayload);
      } catch (error) {
        this.emitter.emit('error', error);
      }
    };

    this.emitter.on('*', listener);

    return () => {
      this.emitter.off('*', listener);
    };
  }

  /**
   * Get event history with optional filtering
   */
  getHistory(filter?: EventFilter): ExecutionEventPayload[] {
    let history = this.eventHistory.slice(); // Return a copy

    if (filter?.executionId) {
      history = history.filter(e => e.executionId === filter.executionId);
    }

    if (filter?.workflowId) {
      history = history.filter(e => e.workflowId === filter.workflowId);
    }

    if (filter?.type) {
      const types = Array.isArray(filter.type) ? filter.type : [filter.type];
      history = history.filter(e => types.includes(e.type));
    }

    if (filter?.startTime) {
      history = history.filter(e => e.timestamp >= filter.startTime!);
    }

    if (filter?.endTime) {
      history = history.filter(e => e.timestamp <= filter.endTime!);
    }

    return history;
  }

  /**
   * Get event count
   */
  getEventCount(filter?: EventFilter): number {
    return this.getHistory(filter).length;
  }

  /**
   * Get events for a specific execution
   */
  getExecutionEvents(executionId: string): ExecutionEventPayload[] {
    return this.getHistory({ executionId });
  }

  /**
   * Get events for a specific workflow
   */
  getWorkflowEvents(workflowId: string): ExecutionEventPayload[] {
    return this.getHistory({ workflowId });
  }

  /**
   * Get events of specific type
   */
  getEventsByType(type: ExecutionEventType | ExecutionEventType[]): ExecutionEventPayload[] {
    return this.getHistory({ type });
  }

  /**
   * Clear event history
   */
  clear(): void {
    this.eventHistory = [];
  }

  /**
   * Clear history for specific execution
   */
  clearExecution(executionId: string): void {
    this.eventHistory = this.eventHistory.filter(e => e.executionId !== executionId);
  }

  /**
   * Get history size
   */
  getHistorySize(): number {
    return this.eventHistory.length;
  }

  /**
   * Set max history size
   */
  setMaxHistorySize(size: number): void {
    if (size < 10) {
      throw new Error('Max history size must be at least 10');
    }
    this.maxHistorySize = size;
    this.trimHistory();
  }

  /**
   * Add event to history and trim if necessary
   */
  private addToHistory(payload: ExecutionEventPayload): void {
    this.eventHistory.push(payload);
    this.trimHistory();
  }

  /**
   * Trim history to max size
   */
  private trimHistory(): void {
    if (this.eventHistory.length > this.maxHistorySize) {
      this.eventHistory = this.eventHistory.slice(-this.maxHistorySize);
    }
  }
}

/**
 * Singleton instance of EventManager
 */
let eventManagerInstance: EventManager | null = null;

/**
 * Get or create EventManager singleton
 */
export function getEventManager(): EventManager {
  if (!eventManagerInstance) {
    eventManagerInstance = new EventManager();
  }
  return eventManagerInstance;
}
