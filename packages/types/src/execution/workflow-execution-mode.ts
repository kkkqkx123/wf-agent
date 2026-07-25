/**
 * Workflow Execution Mode Types
 *
 * Defines how a workflow is dispatched for execution:
 * - blocking: Synchronous, waits for completion and returns the result
 * - detached: Foreground terminal display, non-blocking
 * - background: Runs in background, logs to file
 *
 * This type was extracted from apps/cli-app and apps/server to eliminate
 * duplication (cli-app had a type union, server had an enum).
 * It is deliberately distinct from @wf-agent/runtime's ExecutionMode
 * (which controls app-level context: interactive/headless/programmatic).
 */

/**
 * Workflow execution mode type (string union)
 */
export type WorkflowExecutionMode = 'blocking' | 'detached' | 'background';

/**
 * Workflow execution mode constants for enum-like usage.
 * Provides uppercase keys mapping to lowercase values (e.g. WorkflowExecutionModes.DETACHED = 'detached').
 */
export const WorkflowExecutionModes = {
  BLOCKING: 'blocking',
  DETACHED: 'detached',
  BACKGROUND: 'background',
} as const satisfies Record<string, WorkflowExecutionMode>;
