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

import type { WorkflowExecutionRegistry } from "../registry/workflow-execution-registry.js";
import type { InterruptionType } from "../../shared/types/interruption-types.js";
import { isAborted } from "../../shared/utils/interruption/index.js";
import {
  checkWorkflowInterruption,
  getWorkflowInterruptionType,
} from "./utils/workflow-interruption-utils.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "InterruptionDetector" });

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
      logger.warn("Workflow execution context not found", { executionId });
      return new AbortController().signal;
    }

    // Type-safe access to interruptionManager
    if (
      "interruptionManager" in executionContext &&
      executionContext.interruptionManager &&
      typeof executionContext.interruptionManager === "object" &&
      "getAbortSignal" in executionContext.interruptionManager &&
      typeof executionContext.interruptionManager.getAbortSignal === "function"
    ) {
      return executionContext.interruptionManager.getAbortSignal();
    }

    logger.error("InterruptionManager is missing or invalid in execution context", { executionId });
    throw new Error(`InterruptionManager not properly initialized for execution ${executionId}`);
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
    const result = checkWorkflowInterruption(signal);
    return getWorkflowInterruptionType(result);
  }
}
