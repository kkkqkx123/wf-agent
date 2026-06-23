/**
 * Unified execution result type
 * All Core API execution methods return this type.
 *
 * Based on the `packages/types` `Result` type, additional support for `executionTime` has been added.
 */

import type { Result, SDKError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Execution Result Wrapper
 * Includes Result and Execution Time
 */
export interface ExecutionResult<T> {
  /** Execution result */
  result: Result<T, SDKError>;
  /** Execution time (in milliseconds) */
  executionTime: number;
}

/**
 * Creation successful.
 */
export function success<T>(data: T, executionTime: number): ExecutionResult<T> {
  return {
    result: ok(data),
    executionTime,
  };
}

/**
 * Creation failed.
 */
export function failure<T>(error: SDKError, executionTime: number): ExecutionResult<T> {
  return {
    result: err(error),
    executionTime,
  };
}

/**
 * Did the check result in success?
 */
export function isSuccess<T>(result: ExecutionResult<T>): boolean {
  return result.result.isOk();
}

/**
 * Did the check result in a failure?
 */
export function isFailure<T>(result: ExecutionResult<T>): boolean {
  return result.result.isErr();
}

/**
 * Retrieve the result data (if successful).
 */
export function getData<T>(result: ExecutionResult<T>): T | null {
  return result.result.isOk() ? result.result.value : null;
}

/**
 * Get the error message (in case of failure)
 */
export function getError<T>(result: ExecutionResult<T>): SDKError | null {
  return result.result.isErr() ? result.result.error : null;
}

/**
 * Get the error message (in case of failure)
 */
export function getErrorMessage<T>(result: ExecutionResult<T>): string | null {
  if (!result.result.isErr()) return null;
  return result.result.error.message;
}
