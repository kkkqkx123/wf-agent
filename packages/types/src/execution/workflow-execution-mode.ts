/**
 * Workflow Execution Mode Types
 *
 * Defines how a workflow is dispatched for execution:
 * - blocking: Synchronous, waits for completion and returns the result
 * - foreground: In-process execution with real-time event display via node-pty
 * - background: Runs in background, logs to file
 *
 * This type was extracted from apps/cli-app and apps/server to eliminate
 * duplication (cli-app had a type union, server had an enum).
 * It is deliberately distinct from @wf-agent/runtime's ExecutionMode
 * (which controls app-level context: interactive/headless).
 */

/**
 * Workflow execution mode type (string union)
 *
 * - blocking:   synchronous, waits for completion in the current terminal
 * - foreground: async in-process with real-time event display via node-pty terminal
 * - background: async, log-file output, can outlive CLI via OS-level child process
 */
export type WorkflowExecutionMode = 'blocking' | 'foreground' | 'background';

/**
 * Workflow execution mode constants for enum-like usage.
 * Provides uppercase keys mapping to lowercase values (e.g. WorkflowExecutionModes.FOREGROUND = 'foreground').
 */
export const WorkflowExecutionModes = {
  BLOCKING: 'blocking',
  FOREGROUND: 'foreground',
  BACKGROUND: 'background',
} as const satisfies Record<string, WorkflowExecutionMode>;
