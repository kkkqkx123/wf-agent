/**
 * Checkpoint Event Builders
 * Provides builders for checkpoint-related events
 */

import { createBuilder, createErrorBuilder } from "./common.js";
import type {
  CheckpointCreatedEvent,
  CheckpointRestoredEvent,
  CheckpointDeletedEvent,
  CheckpointFailedEvent,
} from "@wf-agent/types";

// =============================================================================
// Checkpoint Events
// =============================================================================

/**
 * Build checkpoint created event
 */
export const buildCheckpointCreatedEvent =
  createBuilder<CheckpointCreatedEvent>("CHECKPOINT_CREATED");

/**
 * Build checkpoint restored event
 */
export const buildCheckpointRestoredEvent =
  createBuilder<CheckpointRestoredEvent>("CHECKPOINT_RESTORED");

/**
 * Build checkpoint deleted event
 */
export const buildCheckpointDeletedEvent =
  createBuilder<CheckpointDeletedEvent>("CHECKPOINT_DELETED");

/**
 * Build checkpoint failed event
 */
export const buildCheckpointFailedEvent =
  createErrorBuilder<CheckpointFailedEvent>("CHECKPOINT_FAILED");
