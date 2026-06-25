/**
 * Error Converter - Unified error handling using Result pattern
 *
 * Key Design:
 * - No exception throwing in normal flow, only Result values
 * - Errors are values, not control flow
 * - SDK errors are inherited, not wrapped
 * - Supports error collection for validation (multiple errors at once)
 */

import { CommandError } from '@wf-agent/sdk/api';
import type { Result } from '@wf-agent/types';
import { ok, err } from '@wf-agent/common-utils';

/**
 * Kit-specific error codes
 * Extends SDK error codes with Kit-specific scenarios
 */
export enum KitErrorCode {
  // Kit-specific errors
  DUPLICATE_NODE_ID = 'DUPLICATE_NODE_ID',
  NODE_NOT_FOUND = 'NODE_NOT_FOUND',
  INVALID_WORKFLOW = 'INVALID_WORKFLOW',
  EXECUTION_NOT_FOUND = 'EXECUTION_NOT_FOUND',
  VERSION_NOT_FOUND = 'VERSION_NOT_FOUND',

  // SDK-compatible errors (inherit from SDK)
  WORKFLOW_NOT_FOUND = 'WORKFLOW_NOT_FOUND',
  EXECUTION_FAILED = 'EXECUTION_FAILED',
  VALIDATION_ERROR = 'VALIDATION_ERROR',
  TIMEOUT = 'TIMEOUT',
  INTERNAL_ERROR = 'INTERNAL_ERROR',
  RESOURCE_NOT_FOUND = 'RESOURCE_NOT_FOUND',
  PERMISSION_ERROR = 'PERMISSION_ERROR',
}

/**
 * KitError - Inherits from CommandError for consistency with SDK
 *
 * Benefits:
 * - Preserves SDK error hierarchy
 * - Maintains context and severity information
 * - Allows instanceof checks against both KitError and CommandError
 * - No information loss in error conversion
 */
export class KitError extends CommandError {
  public readonly kitErrorCode: KitErrorCode;

  constructor(
    message: string,
    kitErrorCode: KitErrorCode,
    context?: Record<string, unknown>,
    cause?: Error
  ) {
    super(message, kitErrorCode, context, undefined);
    this.kitErrorCode = kitErrorCode;
    Object.setPrototypeOf(this, KitError.prototype);

      // Use Object.defineProperty to set read-only cause property
      if (cause instanceof Error) {
        Object.defineProperty(this, 'cause', {
          value: cause,
          enumerable: true,
          writable: false,
          configurable: false
        });
      }
  }

  override toJSON() {
    return {
      ...super.toJSON(),
      kitErrorCode: this.kitErrorCode,
    };
  }
}

/**
 * ErrorConverter - No exception-based conversion, only Result values
 *
 * Philosophy:
 * - Errors are data, not exceptions
 * - Support batch error collection for validation
 * - Preserve SDK error information
 * - Enable Result chaining
 */
export class ErrorConverter {
  /**
   * Convert SDK Result type to Result<T, KitError>
   *
   * Returns Result, never throws
   */
  convertResult<T>(result: any): Result<T, KitError> {
    // Check if result has the SDK Result structure
    if (result && typeof result === 'object') {
      // Check for success case
      if (this.isSuccessResult(result)) {
        return ok(this.getSuccessData(result));
      }

      // Check for failure case
      if (this.isFailureResult(result)) {
        const sdkError = this.getErrorData(result);
        return err(this.toKitError(sdkError));
      }
    }

    // If it doesn't look like a Result type, return as-is
    return ok(result);
  }

  /**
   * Convert any error to KitError, preserving SDK error chain
   *
   * Returns KitError, preserving error information
   */
  toKitError(error: any): KitError {
    if (error instanceof KitError) {
      return error;
    }

    // If already a CommandError from SDK, wrap but preserve
    if (error instanceof CommandError) {
      return new KitError(
        error.message,
        (error.code as KitErrorCode) || KitErrorCode.INTERNAL_ERROR,
        error.context,
        error instanceof Error ? error : undefined
      );
    }

    // Generic error handling
    const message = error?.message || 'Unknown error';
    const code = this.detectErrorCode(error);

    return new KitError(
      message,
      code,
      this.extractContext(error),
      error instanceof Error ? error : undefined
    );
  }

  /**
   * Batch validation error collection
   *
   * Returns Result with all collected errors
   */
  collectValidationErrors(
    validations: Array<{
      field: string;
      validator: () => Result<void, KitError>;
    }>
  ): Result<void, KitError[]> {
    const errors: KitError[] = [];

    for (const { validator } of validations) {
      const result = validator();
      if (result.isErr()) {
        const error = result.unwrapOrElse(e => e);
        errors.push(error);
      }
    }

    if (errors.length > 0) {
      return err(errors);
    }

    return ok(undefined);
  }

  /**
   * Detect error code from various error types
   */
  private detectErrorCode(error: any): KitErrorCode {
    if (!error) {
      return KitErrorCode.INTERNAL_ERROR;
    }

    const message = error.message?.toLowerCase() || '';
    const code = error.code?.toUpperCase() || '';

    // Try to match SDK error codes
    if (code.includes('NOT_FOUND') || message.includes('not found')) {
      return KitErrorCode.WORKFLOW_NOT_FOUND;
    }
    if (code.includes('VALIDATION') || message.includes('validation')) {
      return KitErrorCode.VALIDATION_ERROR;
    }
    if (code.includes('TIMEOUT') || message.includes('timeout')) {
      return KitErrorCode.TIMEOUT;
    }
    if (code.includes('EXECUTION') || message.includes('execution')) {
      return KitErrorCode.EXECUTION_FAILED;
    }
    if (code.includes('PERMISSION') || message.includes('permission')) {
      return KitErrorCode.PERMISSION_ERROR;
    }

    return KitErrorCode.INTERNAL_ERROR;
  }

  /**
   * Extract context from error object
   */
  private extractContext(error: any): Record<string, unknown> {
    if (error && typeof error === 'object') {
      if (error.context) {
        return error.context;
      }
      return { originalError: String(error) };
    }
    return {};
  }

  /**
   * Check if result is a success (follows SDK Result type structure)
   */
  private isSuccessResult(result: any): boolean {
    return result && result.success === true;
  }

  /**
   * Check if result is a failure (follows SDK Result type structure)
   */
  private isFailureResult(result: any): boolean {
    return result && result.success === false;
  }

  /**
   * Extract success data from Result
   */
  private getSuccessData<T>(result: any): T {
    return result.data as T;
  }

  /**
   * Extract error from Result
   */
  private getErrorData(result: any): any {
    return result.error;
  }
}
