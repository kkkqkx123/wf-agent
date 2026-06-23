/**
 * Common types used across SDK-Kit
 */

/**
 * Execution result
 */
export interface ExecutionResult {
  executionId: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  output?: Record<string, unknown>;
  duration?: number;
}

/**
 * Execution record for queries
 */
export interface ExecutionRecord {
  executionId: string;
  workflowId: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  input?: Record<string, unknown>;
  output?: Record<string, unknown>;
  error?: string;
  startTime: number;
  endTime?: number;
  duration?: number;
}

/**
 * Execution options
 */
export interface ExecutionOptions {
  timeout?: number;
  retryCount?: number;
  retryDelayMs?: number;
}

/**
 * Filter criteria for queries
 */
export interface FilterCriteria {
  workflowId?: string;
  status?: string | string[];
  startTime?: { from?: number; to?: number };
  tags?: string[];
  custom?: Record<string, unknown>;
}

/**
 * Sort options for queries
 */
export interface SortOptions {
  field: string;
  order: 'asc' | 'desc';
}

/**
 * Pagination options
 */
export interface PaginationOptions {
  limit: number;
  offset: number;
}

/**
 * Enhanced execution event types
 */
export type ExecutionEventType =
  | 'start'
  | 'progress'
  | 'node_started'
  | 'node_completed'
  | 'node_failed'
  | 'completed'
  | 'error'
  | 'cancelled'
  | 'timeout'
  | 'retry';

/**
 * Enhanced execution event
 */
export interface ExecutionEvent {
  type: ExecutionEventType;
  executionId: string;
  timestamp: number;
  data?: Record<string, unknown>;
  nodeId?: string;
  retryCount?: number;
  duration?: number;
}

/**
 * Event filter criteria
 */
export interface EventFilterCriteria {
  types?: ExecutionEventType[];
  executionIds?: string[];
  startTime?: number;
  endTime?: number;
}

/**
 * Event history entry
 */
export interface EventHistoryEntry {
  event: ExecutionEvent;
  recordedAt: number;
}

/**
 * Execution context for building chains
 */
export interface ExecutionContext {
  workflowId?: string;
  input?: Record<string, unknown>;
  options?: ExecutionOptions;
  executionId?: string;
}
