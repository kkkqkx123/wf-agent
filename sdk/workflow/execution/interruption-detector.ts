/**
 * Interrupt Detector
 * Provides a unified interrupt detection interface
 *
 * Responsibilities:
 * - Determine whether a workflow execution should be interrupted
 * - Retrieve the type of the interrupt
 * - Obtain the AbortSignal
 *
 * Design Principles:
 * - Unified interface: All components use the same detection interface
 * - Dependency injection: Obtain WorkflowExecutionContext through WorkflowExecutionRegistry
 * - Efficiency: Avoid unnecessary object creation
 * - Unified use of AbortSignal as the primary interrupt mechanism
 */

import type { WorkflowExecutionRegistry } from "../stores/workflow-execution-registry.js";
import type { InterruptionType } from "@wf-agent/types";
import { isAborted, checkInterruption, getInterruptionType } from "@wf-agent/common-utils";

/**
 * Interrupt Detector Interface
 */
export interface InterruptionDetector {
  /**
   * Get the AbortSignal
   * @param executionId: Execution ID
   * @returns: AbortSignal
   */
  getAbortSignal(executionId: string): AbortSignal;

  /**
   * Check if it has been terminated.
   * @param executionId: Execution ID
   * @returns: Whether it has been terminated
   */
  isAborted(executionId: string): boolean;

  /**
   * Get interrupt type
   * @param executionId Execution ID
   * @returns Interrupt type (PAUSE/STOP/null)
   */
  getInterruptionType(executionId: string): InterruptionType;
}

/**
 * Interrupt Detector Implementation
 */
export class InterruptionDetectorImpl implements InterruptionDetector {
  constructor(private workflowExecutionRegistry: WorkflowExecutionRegistry) {}

  /**
   * Get the AbortSignal
   * @param executionId: Execution ID
   * @returns: AbortSignal
   */
  getAbortSignal(executionId: string): AbortSignal {
    const executionContext = this.workflowExecutionRegistry.get(executionId);
    if (!executionContext) {
      return new AbortController().signal;
    }

    const interruptionManager = (
      executionContext as { interruptionManager?: { getAbortSignal: () => AbortSignal } }
    ).interruptionManager;
    if (!interruptionManager) {
      return new AbortController().signal;
    }

    return interruptionManager.getAbortSignal();
  }

  /**
   * Check if it has been terminated.
   * @param executionId: Execution ID
   * @returns: Whether it has been terminated
   */
  isAborted(executionId: string): boolean {
    const signal = this.getAbortSignal(executionId);
    return isAborted(signal);
  }

  /**
   * Get interrupt type
   * @param executionId Execution ID
   * @returns Interrupt type (PAUSE/STOP/null)
   */
  getInterruptionType(executionId: string): InterruptionType {
    const signal = this.getAbortSignal(executionId);
    const result = checkInterruption(signal);
    return getInterruptionType(result);
  }
}
