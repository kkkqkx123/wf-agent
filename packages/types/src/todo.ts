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
