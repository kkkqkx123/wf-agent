/**
 * Promise Callback Event Type Definitions
 * 
 * Events related to Promise resolution lifecycle management
 */

import type { BaseEvent } from "./base.js";

/**
 * Promise Callback Registered Event
 */
export interface PromiseCallbackRegisteredEvent extends BaseEvent {
  type: "PROMISE_CALLBACK_REGISTERED";
  executionId: string;
}

/**
 * Promise Callback Resolved Event
 */
export interface PromiseCallbackResolvedEvent extends BaseEvent {
  type: "PROMISE_CALLBACK_RESOLVED";
  executionId: string;
}

/**
 * Promise Callback Rejected Event
 */
export interface PromiseCallbackRejectedEvent extends BaseEvent {
  type: "PROMISE_CALLBACK_REJECTED";
  executionId: string;
  errorMessage?: string;
}

/**
 * Promise Callback Failed Event
 */
export interface PromiseCallbackFailedEvent extends BaseEvent {
  type: "PROMISE_CALLBACK_FAILED";
  executionId: string;
  error?: string;
}

/**
 * Promise Callback Cleaned Up Event
 */
export interface PromiseCallbackCleanedUpEvent extends BaseEvent {
  type: "PROMISE_CALLBACK_CLEANED_UP";
  executionId: string;
  reason?: "global_cleanup" | "individual_cleanup" | "timeout" | "error";
}

/**
 * Union type of all Promise callback events
 */
export type PromiseCallbackEvent =
  | PromiseCallbackRegisteredEvent
  | PromiseCallbackResolvedEvent
  | PromiseCallbackRejectedEvent
  | PromiseCallbackFailedEvent
  | PromiseCallbackCleanedUpEvent;
