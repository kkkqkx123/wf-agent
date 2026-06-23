/**
 * Execution-related types for SDK-Kit
 */

import type { ExecutionEvent, ExecutionResult } from './common.types.js';
import type { Result } from '@wf-agent/common-utils';
import type { KitError } from '../converters/error.converter.js';

/**
 * Execution builder interface
 *
 * Methods return Result for functional error handling
 */
export interface ExecutionBuilder {
  input(data: Record<string, unknown>): ExecutionBuilder;
  onProgress(handler: (event: ExecutionEvent) => void): ExecutionBuilder;
  onError(handler: (error: Error) => void): ExecutionBuilder;
  /**
   * Execute workflow and return Result
   *
   * Returns Result<ExecutionResult, KitError> for error handling
   */
  execute(): Promise<Result<ExecutionResult, KitError>>;
  getExecutionId(): string;
}

/**
 * Event handler function
 */
export type EventHandler = (event: ExecutionEvent | Error) => void;
