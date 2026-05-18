/**
 * Internal Interruption Types for SDK
 * 
 * These types are internal implementation details of the SDK's interruption handling mechanism.
 * They should NOT be exported from the public types package.
 * 
 * Design Principles:
 * - Interrupt handling is an internal control flow mechanism
 * - External applications use ExecutionInterruptionCheckResult for interruption information
 * - Exception classes are implementation details that may change frequently
 */

import { SDKError, ErrorSeverity } from "@wf-agent/types";

/**
 * Interrupt Type
 * @internal SDK internal use only
 */
export type InterruptionType = "PAUSE" | "STOP" | null;

/**
 * Interrupt Exception Base Class
 * 
 * @description
 * 1. Generic execution interrupt exception base class
 * 2. This is a control flow exception, not a real error
 * 3. Interrupt type: PAUSE (pause, resume) or STOP (stop, not resume)
 * 4. Subclasses can add module-specific context information
 * 
 * @internal SDK internal use only
 */
export class InterruptedException extends SDKError {
  constructor(
    message: string,
    public readonly interruptionType: InterruptionType,
    context?: Record<string, unknown>,
  ) {
    super(message, "info", { ...context, interruptionType });
  }

  protected override getDefaultSeverity(): ErrorSeverity {
    return "info";
  }
}

/**
 * AbortError - Operation Abort Error
 * 
 * @description
 * 1. Thrown when AbortSignal is triggered
 * 2. This is a control flow exception, not a real error
 * 3. Contains the original cause of the interruption (InterruptedException or its subclasses)
 * 
 * @internal SDK internal use only
 */
export class AbortError extends Error {
  public override readonly name = "AbortError";

  constructor(
    message: string,
    public override readonly cause?: InterruptedException,
  ) {
    super(message);
    // Maintaining the right prototype chain
    Object.setPrototypeOf(this, AbortError.prototype);
  }
}
