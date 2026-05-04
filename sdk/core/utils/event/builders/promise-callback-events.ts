/**
 * Promise Callback Event Builders
 * Provides builder functions for promise callback lifecycle events
 */

import { now, generateId } from "@wf-agent/common-utils";
import {
  createBuilder,
} from "./common.js";
import type {
  PromiseCallbackRegisteredEvent,
  PromiseCallbackResolvedEvent,
  PromiseCallbackRejectedEvent,
  PromiseCallbackFailedEvent,
  PromiseCallbackCleanedUpEvent,
} from "@wf-agent/types";
import type { BuildParams } from "./common.js";

/**
 * Build PROMISE_CALLBACK_REGISTERED event
 */
export const buildPromiseCallbackRegisteredEvent =
  createBuilder<PromiseCallbackRegisteredEvent>("PROMISE_CALLBACK_REGISTERED");

/**
 * Build PROMISE_CALLBACK_RESOLVED event
 */
export const buildPromiseCallbackResolvedEvent =
  createBuilder<PromiseCallbackResolvedEvent>("PROMISE_CALLBACK_RESOLVED");

/**
 * Build PROMISE_CALLBACK_REJECTED event with Error transformation to string
 */
export const buildPromiseCallbackRejectedEvent = (
  params: Omit<BuildParams<PromiseCallbackRejectedEvent>, "errorMessage"> & { error?: Error },
): PromiseCallbackRejectedEvent => ({
  id: generateId(),
  type: "PROMISE_CALLBACK_REJECTED",
  timestamp: now(),
  ...params,
  errorMessage: params.error?.message,
});

/**
 * Build PROMISE_CALLBACK_FAILED event with Error transformation to string
 */
export const buildPromiseCallbackFailedEvent = (
  params: Omit<BuildParams<PromiseCallbackFailedEvent>, "error"> & { error?: Error },
): PromiseCallbackFailedEvent => ({
  id: generateId(),
  type: "PROMISE_CALLBACK_FAILED",
  timestamp: now(),
  ...params,
  error: params.error?.message,
});

/**
 * Build PROMISE_CALLBACK_CLEANED_UP event
 */
export const buildPromiseCallbackCleanedUpEvent =
  createBuilder<PromiseCallbackCleanedUpEvent>("PROMISE_CALLBACK_CLEANED_UP");
