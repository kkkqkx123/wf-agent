/**
 * Event Builders - Unified Export
 * Provides all event builder functions from a single entry point
 *
 * Usage:
 * ```typescript
 * import {
 *   buildMessageAddedEvent,
 *   buildThreadStartedEvent,
 *   buildErrorEvent
 * } from './index.js';
 * ```
 */

// Common utilities (for advanced usage)
export * from "./common.js";

// LLM events
export * from "./llm-events.js";

// Error events
export * from "./error-events.js";

// Workflow Execution events
export * from "./workflow-execution-events.js";

// Node events
export * from "./node-events.js";

// Tool events
export * from "./tool-events.js";

// Subgraph events
export * from "./subgraph-events.js";

// Checkpoint events
export * from "./checkpoint-events.js";

// Interaction events
export * from "./interaction-events.js";

// System events
export * from "./system-events.js";

// Skill events
export * from "./skill-events.js";

// Hook events
export * from "./hook-events.js";

// Custom events
export * from "./custom-events.js";

// =============================================================================
// Backward Compatibility Aliases
// =============================================================================

// Re-export new functions with old names for backward compatibility
export {
  buildWorkflowExecutionStartedEvent as buildThreadStartedEvent,
  buildWorkflowExecutionCompletedEvent as buildThreadCompletedEvent,
  buildWorkflowExecutionFailedEvent as buildThreadFailedEvent,
  buildWorkflowExecutionPausedEvent as buildThreadPausedEvent,
  buildWorkflowExecutionResumedEvent as buildThreadResumedEvent,
  buildWorkflowExecutionCancelledEvent as buildThreadCancelledEvent,
  buildWorkflowExecutionStateChangedEvent as buildThreadStateChangedEvent,
  buildWorkflowExecutionForkStartedEvent as buildThreadForkStartedEvent,
  buildWorkflowExecutionForkCompletedEvent as buildThreadForkCompletedEvent,
  buildWorkflowExecutionJoinStartedEvent as buildThreadJoinStartedEvent,
  buildWorkflowExecutionJoinConditionMetEvent as buildThreadJoinConditionMetEvent,
  buildWorkflowExecutionCopyStartedEvent as buildThreadCopyStartedEvent,
  buildWorkflowExecutionCopyCompletedEvent as buildThreadCopyCompletedEvent,
} from "./workflow-execution-events.js";
