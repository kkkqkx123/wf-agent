/**
 * Attempt Completion Event Builders
 * Provides builders for attempt_completion-related events
 */

import { createBuilder } from "./common.js";
import type { AttemptCompletionEvent } from "@wf-agent/types";

/**
 * Build attempt completion event
 *
 * Emitted when an LLM calls the attempt_completion tool
 * during an agent loop iteration.
 */
export const buildAttemptCompletionEvent =
  createBuilder<AttemptCompletionEvent>("ATTEMPT_COMPLETION");