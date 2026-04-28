/**
 * Workflow State Transition Verification Tool Function
 *
 * Responsibilities:
 * - Define the allowed state transition rules
 * - Provide an interface for state transition verification
 * - Offer functions for querying workflow states
 *
 * Design Principles:
 * - Pure functions: No side effects; the same input always produces the same output
 * - Reusability: To be shared by WorkflowLifecycleCoordinator and WorkflowStateTransitor
 * - Simplicity: Export specific functions rather than creating a class
 */

import type { WorkflowExecutionStatus } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";

/**
 * State transition rule definitions:
 * CREATED -> RUNNING
 * RUNNING -> PAUSED | COMPLETED | FAILED | CANCELLED | TIMEOUT
 * PAUSED -> RUNNING | CANCELLED | TIMEOUT
 * COMPLETED/FAILED/CANCELLED/TIMEOUT -> Final state; cannot be transitioned
 */
const STATE_TRANSITIONS: Record<string, string[]> = {
  CREATED: ["RUNNING"],
  RUNNING: ["PAUSED", "COMPLETED", "FAILED", "CANCELLED", "TIMEOUT"],
  PAUSED: ["RUNNING", "CANCELLED", "TIMEOUT"],
  COMPLETED: [],
  FAILED: [],
  CANCELLED: [],
  TIMEOUT: [],
};

/**
 * Check if the status transition is valid
 *
 * @param currentStatus: Current status
 * @param targetStatus: Target status
 * @returns: Whether the transition is allowed
 */
export function isValidTransition(
  currentStatus: WorkflowExecutionStatus,
  targetStatus: WorkflowExecutionStatus,
): boolean {
  const allowedTransitions = STATE_TRANSITIONS[currentStatus] || [];
  return allowedTransitions.includes(targetStatus);
}

/**
 * Verify the status transition; throw an error if it is invalid.
 *
 * @param executionId Workflow Execution ID (used for error messages)
 * @param currentStatus Current status
 * @param targetStatus Target status
 * @throws ValidationError The status transition is invalid.
 */
export function validateTransition(
  workflowExecutionId: string,
  currentStatus: WorkflowExecutionStatus,
  targetStatus: WorkflowExecutionStatus,
): void {
  if (!isValidTransition(currentStatus, targetStatus)) {
    throw new RuntimeValidationError(
      `Invalid state transition: ${currentStatus} -> ${targetStatus}`,
      {
        operation: "validateStateTransition",
        field: "workflowExecution.status",
        value: { currentStatus, targetStatus },
      },
    );
  }
}

/**
 * Get the target states allowed by the current status
 *
 * @param currentStatus: The current status
 * @returns: An array of target states that are allowed
 */
export function getAllowedTransitions(currentStatus: WorkflowExecutionStatus): WorkflowExecutionStatus[] {
  return (STATE_TRANSITIONS[currentStatus] || []) as WorkflowExecutionStatus[];
}

/**
 * Check if the status is in a terminated state.
 *
 * @param status: The status
 * @returns: Whether it is in a terminated state
 */
export function isTerminalStatus(status: WorkflowExecutionStatus): boolean {
  return ["COMPLETED", "FAILED", "CANCELLED", "TIMEOUT"].includes(status);
}

/**
 * Check if the status is active (can be interrupted)
 *
 * @param status The status
 * @returns Whether it is an active status
 */
export function isActiveStatus(status: WorkflowExecutionStatus): boolean {
  return ["RUNNING", "PAUSED"].includes(status);
}
