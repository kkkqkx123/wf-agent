/**
 * TODO List Type Definition
 *
 * Define the core business types associated with TODO lists
 * for task tracking, progress management and agent collaboration
 */

import type { ID, Metadata } from "./common.js";

/**
 * TODO item status
 */
export type TodoStatus = "pending" | "in_progress" | "completed" | "cancelled";

/**
 * TODO Item Priority
 */
export type TodoPriority = "high" | "medium" | "low";

/**
 * TODO list item
 */
export interface TodoItem {
  /**
   * TODO ID
   * Should be unique within the session
   */
  id: ID;

  /**
   * TODO Content
   * Succinctly describe the task content
   */
  content: string;

  /**
   * TODO Status
   */
  status: TodoStatus;

  /**
   * Priority (optional)
   */
  priority?: TodoPriority;

  /**
   * Create timestamp (optional)
   */
  createdAt?: number;

  /**
   * Update timestamp (optional)
   */
  updatedAt?: number;

  /**
   * Metadata (optional)
   * Additional business information can be stored
   */
  metadata?: Metadata;
}

/**
 * TODO Write Parameters
 * Used to create or completely replace a TODO list
 */
export interface TodoWriteArgs {
  /**
   * TODO Item List
   */
  todos: TodoItem[];
}

/**
 * TODO Updating Operation Types
 * Using Discriminative Union Types to Ensure Type Safety
 */
export type TodoUpdateOp =
  /** New or upsert (update if id already exists) */
  | { op: "add"; id: ID; content: string; status?: TodoStatus; priority?: TodoPriority }
  /** Update Status */
  | { op: "set_status"; id: ID; status: TodoStatus }
  /** Updates */
  | { op: "set_content"; id: ID; content: string }
  /** Update Priority */
  | { op: "set_priority"; id: ID; priority: TodoPriority }
  /** Mark as canceled */
  | { op: "cancel"; id: ID }
  /** Remove from list */
  | { op: "remove"; id: ID };

/**
 * TODO Update Parameters
 * Used to perform incremental updates to the TODO list
 */
export interface TodoUpdateArgs {
  /**
   * Update Action List
   */
  ops: TodoUpdateOp[];
}

/**
 * TODO list statistics
 */
export interface TodoStats {
  /**
   * aggregate
   */
  total: number;

  /**
   * Counting of each state
   */
  counts: Record<TodoStatus, number>;

  /**
   * Count of each priority (optional)
   */
  priorityCounts?: Record<TodoPriority, number>;
}

/**
 * TODO Write result
 */
export interface TodoWriteResult {
  /**
   * success or failure
   */
  success: boolean;

  /**
   * Statistical information
   */
  stats: TodoStats;

  /**
   * Error message (on failure)
   */
  error?: string;
}

/**
 * TODO Updated results
 */
export interface TodoUpdateResult {
  /**
   * success or failure
   */
  success: boolean;

  /**
   * Applied operands
   */
  appliedOps: number;

  /**
   * Number of additions
   */
  added: number;

  /**
   * Number of updates
   */
  updated: number;

  /**
   * Number of cancellations
   */
  cancelled: number;

  /**
   * Number of removals
   */
  removed: number;

  /**
   * invalid operand
   */
  invalidOps: number;

  /**
   * List of IDs not found
   */
  notFoundIds: ID[];

  /**
   * Statistical information
   */
  stats: TodoStats;

  /**
   * Error message (on failure)
   */
  error?: string;
}

/**
 * TODO list configuration
 */
export interface TodoConfig {
  /**
   * Maximum number of items
   */
  maxItems?: number;

  /**
   * Whether to enable prioritization
   */
  enablePriority?: boolean;

  /**
   * Default Priority
   */
  defaultPriority?: TodoPriority;

  /**
   * Whether to allow duplicate IDs
   */
  allowDuplicateIds?: boolean;
}

/**
 * TODO List Validation Error
 */
export class TodoValidationError extends Error {
  constructor(
    public readonly field: string,
    public readonly reason: string,
  ) {
    super(`TODO validation failed for ${field}: ${reason}`);
    this.name = "TodoValidationError";
  }
}

/**
 * TODO List operation error
 */
export class TodoOperationError extends Error {
  constructor(
    public readonly operation: string,
    public readonly id: ID | undefined,
    public readonly reason: string,
  ) {
    super(`TODO operation '${operation}' failed${id ? ` for id '${id}'` : ""}: ${reason}`);
    this.name = "TodoOperationError";
  }
}
