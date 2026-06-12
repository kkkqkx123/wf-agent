/**
 * Async Completion Event Builders
 * Provides builder functions for async completion lifecycle events
 */

import { createBuilder } from "./common.js";
import type {
  AsyncCompletionRegisteredEvent,
  AsyncCompletionTriggeredEvent,
  AsyncCompletionErrorTriggeredEvent,
  AsyncCompletionFailedEvent,
  AsyncCompletionCleanedUpEvent,
} from "@wf-agent/types";
import type { BuildParams } from "./common.js";

// =============================================================================
// Internal builder instances
// =============================================================================

const _buildErrorTriggered = createBuilder<AsyncCompletionErrorTriggeredEvent>("ASYNC_COMPLETION_ERROR_TRIGGERED");
const _buildFailed = createBuilder<AsyncCompletionFailedEvent>("ASYNC_COMPLETION_FAILED");

/**
 * Build ASYNC_COMPLETION_REGISTERED event
 */
export const buildAsyncCompletionRegisteredEvent =
  createBuilder<AsyncCompletionRegisteredEvent>("ASYNC_COMPLETION_REGISTERED");

/**
 * Build ASYNC_COMPLETION_TRIGGERED event
 */
export const buildAsyncCompletionTriggeredEvent =
  createBuilder<AsyncCompletionTriggeredEvent>("ASYNC_COMPLETION_TRIGGERED");

/**
 * Build ASYNC_COMPLETION_ERROR_TRIGGERED event with Error transformation to string
 */
export const buildAsyncCompletionErrorTriggeredEvent = (
  params: Omit<BuildParams<AsyncCompletionErrorTriggeredEvent>, "errorMessage"> & { error?: Error },
): AsyncCompletionErrorTriggeredEvent =>
  _buildErrorTriggered({
    executionId: params.executionId,
    errorMessage: params.error?.message,
  });

/**
 * Build ASYNC_COMPLETION_FAILED event with Error transformation to string
 */
export const buildAsyncCompletionFailedEvent = (
  params: Omit<BuildParams<AsyncCompletionFailedEvent>, "error"> & { error?: Error },
): AsyncCompletionFailedEvent =>
  _buildFailed({
    executionId: params.executionId,
    error: params.error?.message,
  });

/**
 * Build ASYNC_COMPLETION_CLEANED_UP event
 */
export const buildAsyncCompletionCleanedUpEvent =
  createBuilder<AsyncCompletionCleanedUpEvent>("ASYNC_COMPLETION_CLEANED_UP");