/**
 * Async Completion Event Type Definitions
 * 
 * Events related to async task completion lifecycle management
 */

import type { BaseEvent } from "./base.js";

/**
 * Async Completion Registered Event
 */
export interface AsyncCompletionRegisteredEvent extends BaseEvent {
  type: "ASYNC_COMPLETION_REGISTERED";
  executionId: string;
}

/**
 * Async Completion Triggered Event
 */
export interface AsyncCompletionTriggeredEvent extends BaseEvent {
  type: "ASYNC_COMPLETION_TRIGGERED";
  executionId: string;
}

/**
 * Async Completion Error Triggered Event
 */
export interface AsyncCompletionErrorTriggeredEvent extends BaseEvent {
  type: "ASYNC_COMPLETION_ERROR_TRIGGERED";
  executionId: string;
  errorMessage?: string;
}

/**
 * Async Completion Failed Event
 */
export interface AsyncCompletionFailedEvent extends BaseEvent {
  type: "ASYNC_COMPLETION_FAILED";
  executionId: string;
  error?: string;
}

/**
 * Async Completion Cleaned Up Event
 */
export interface AsyncCompletionCleanedUpEvent extends BaseEvent {
  type: "ASYNC_COMPLETION_CLEANED_UP";
  executionId: string;
  reason?: "global_cleanup" | "individual_cleanup" | "timeout" | "error";
}

/**
 * Union type of all Async Completion events
 */
export type AsyncCompletionEvent =
  | AsyncCompletionRegisteredEvent
  | AsyncCompletionTriggeredEvent
  | AsyncCompletionErrorTriggeredEvent
  | AsyncCompletionFailedEvent
  | AsyncCompletionCleanedUpEvent;