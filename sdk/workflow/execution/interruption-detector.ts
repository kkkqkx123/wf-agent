/**
 * Interrupt Detector
 * Provides a unified interrupt detection interface
 *
 * Responsibilities:
 * - Determine whether a thread should be interrupted
 * - Retrieve the type of the interrupt
 * - Obtain the AbortSignal
 *
 * Design Principles:
 * - Unified interface: All components use the same detection interface
 * - Dependency injection: Obtain ThreadContext through ThreadRegistry
 * - Efficiency: Avoid unnecessary object creation
 * - Unified use of AbortSignal as the primary interrupt mechanism
 */

import type { ThreadRegistry } from "../stores/thread-registry.js";
import type { InterruptionType } from "@wf-agent/types";
import { isAborted, checkInterruption, getInterruptionType } from "@wf-agent/common-utils";

/**
 * Interrupt Detector Interface
 */
export interface InterruptionDetector {
  /**
   * Get the AbortSignal
   * @param threadId: Thread ID
   * @returns: AbortSignal
   */
  getAbortSignal(threadId: string): AbortSignal;

  /**
   * Check if it has been terminated.
   * @param threadId: Thread ID
   * @returns: Whether it has been terminated
   */
  isAborted(threadId: string): boolean;

  /**
   * Get interrupt type
   * @param threadId Thread ID
   * @returns Interrupt type (PAUSE/STOP/null)
   */
  getInterruptionType(threadId: string): InterruptionType;
}

/**
 * Interrupt Detector Implementation
 */
export class InterruptionDetectorImpl implements InterruptionDetector {
  constructor(private threadRegistry: ThreadRegistry) {}

  /**
   * Get the AbortSignal
   * @param threadId: Thread ID
   * @returns: AbortSignal
   */
  getAbortSignal(threadId: string): AbortSignal {
    const threadContext = this.threadRegistry.get(threadId);
    if (!threadContext) {
      return new AbortController().signal;
    }

    const interruptionManager = (
      threadContext as { interruptionManager?: { getAbortSignal: () => AbortSignal } }
    ).interruptionManager;
    if (!interruptionManager) {
      return new AbortController().signal;
    }

    return interruptionManager.getAbortSignal();
  }

  /**
   * Check if it has been terminated.
   * @param threadId: Thread ID
   * @returns: Whether it has been terminated
   */
  isAborted(threadId: string): boolean {
    const signal = this.getAbortSignal(threadId);
    return isAborted(signal);
  }

  /**
   * Get interrupt type
   * @param threadId Thread ID
   * @returns Interrupt type (PAUSE/STOP/null)
   */
  getInterruptionType(threadId: string): InterruptionType {
    const signal = this.getAbortSignal(threadId);
    const result = checkInterruption(signal);
    return getInterruptionType(result);
  }
}
