/**
 * Shared checkpoint error handling utilities
 *
 * Extracted from duplicated logic in CheckpointCoordinator (workflow) and
 * AgentLoopCheckpointCoordinator (agent) to reduce code duplication.
 */

import type { CheckpointErrorHandler } from "../hierarchy/error-handler.js";

/**
 * Handle file checkpoint errors with configurable error strategy.
 *
 * When a CheckpointErrorHandler is provided, delegates to it for
 * configurable error handling (rethrow, warn, or ignore).
 * When no handler is available, falls back to the provided fallback behavior.
 *
 * @param error - The error that occurred
 * @param operation - The operation being performed (create or restore)
 * @param entityId - The entity ID for logging context
 * @param options - Optional configuration
 * @param options.checkpointErrorHandler - The error handler to use
 * @param options.checkpointId - The checkpoint ID for logging context
 * @param options.fallbackBehavior - Fallback behavior when no handler is configured (default: "warn")
 * @param options.entityLabel - Label for the entity type in log messages (default: "entity")
 * @throws The original error if the strategy dictates rethrow or fallback is "error"
 */
export async function handleFileCheckpointError(
  error: Error,
  operation: "create" | "restore",
  entityId: string,
  options: {
    checkpointErrorHandler?: CheckpointErrorHandler;
    checkpointId?: string;
    fallbackBehavior?: "error" | "warn" | "ignore";
    entityLabel?: string;
  } = {},
): Promise<void> {
  const {
    checkpointErrorHandler,
    checkpointId = "unknown",
    fallbackBehavior = "warn",
    entityLabel = "entity",
  } = options;

  if (checkpointErrorHandler) {
    const result = await checkpointErrorHandler.handleError(error, {
      operation: "create",
      checkpointId,
      entityId,
      triggerEvent: `file_checkpoint_${operation}`,
      timestamp: Date.now(),
    });
    if (result.shouldRethrow) {
      throw error;
    }
    return;
  }

  // Fallback behavior when no handler is configured
  if (fallbackBehavior === "error") {
    throw error;
  }
  if (fallbackBehavior === "warn") {
    const operationText = operation === "create" ? "creation" : "restore";
    console.warn(
      `[Checkpoint] File checkpoint ${operationText} failed (non-fatal) for ${entityLabel}: ${entityId}`,
      error.message,
    );
  }
  // "ignore" behavior: silently continue
}