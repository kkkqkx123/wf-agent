/**
 * Attempt Completion Event Type Definitions
 *
 * Events related to the attempt_completion tool lifecycle.
 * Emitted when an LLM calls attempt_completion to signal task completion.
 */

import type { BaseEvent } from "./base.js";

/**
 * Attempt Completion Event
 *
 * Carries the completion data from an attempt_completion tool call
 * for trigger matching and variable synchronization.
 */
export interface AttemptCompletionEvent extends BaseEvent {
  type: "ATTEMPT_COMPLETION";
  /** The LLM's final assistant message content (result text) */
  content: string;
  /** Structured output records to append to array variables */
  data?: Record<string, unknown>;
  /** State variables to set directly */
  variables?: Record<string, unknown>;
  /** Node ID where the agent loop is running */
  nodeId?: string;
}
