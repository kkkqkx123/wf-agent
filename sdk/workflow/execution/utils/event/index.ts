/**
 * Event utility class module export
 * Provides utility functions related to events
 *
 * New consumers should import builders directly from core/utils/event/builders.
 * This file only exports event-waiting utilities and the emit function.
 */

// Event Trigger Tool Function (re-exported from core)
export { emit } from "../../../../core/utils/event/emit-event.js";

// Event Waiting Tool Function
export {
  waitForWorkflowExecutionPaused,
  waitForWorkflowExecutionCancelled,
  waitForWorkflowExecutionCompleted,
  waitForWorkflowExecutionFailed,
  waitForWorkflowExecutionResumed,
  waitForAnyLifecycleEvent,
  waitForMultipleWorkflowExecutionsCompleted,
  waitForAnyWorkflowExecutionCompleted,
  waitForAnyWorkflowExecutionCompletion,
  waitForNodeCompleted,
  waitForNodeFailed,
} from "./event-waiter.js";
