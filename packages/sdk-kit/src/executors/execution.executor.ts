/**
 * Execution Runner - Simplified workflow execution using Result pattern
 *
 * Design:
 * - Methods return Result<T, KitError>
 * - No exception throwing in normal operations
 * - SDK errors automatically converted to KitError
 * - Events emitted for monitoring execution lifecycle
 */

import { EventEmitter } from 'node:events';
import { ErrorConverter, KitError, KitErrorCode } from '../converters/error.converter.js';
import type { ExecutionResult, ExecutionContext, ExecutionEvent, ExecutionOptions } from '../types/common.types.js';
import type { ExecutionBuilder } from '../types/execution.types.js';
import type { ExecuteWorkflowCommandConstructor } from '../types/sdk.types.js';
import type { Result } from '@wf-agent/types';
import { ok, err } from '@wf-agent/common-utils';

// Type-only import to avoid runtime dependency issues
type SDKInstance = any;

/**
 * Execution Runner implementation
 */
export class ExecutionRunner {
  private errorConverter: ErrorConverter;
  private eventEmitter: EventEmitter;
  private sdk: SDKInstance;
  private executeCommand: ExecuteWorkflowCommandConstructor;

  constructor(sdk: SDKInstance, executeCommand: ExecuteWorkflowCommandConstructor) {
    this.sdk = sdk;
    this.executeCommand = executeCommand;
    this.errorConverter = new ErrorConverter();
    this.eventEmitter = new EventEmitter();
  }

  /**
   * Execute a workflow
   *
   * Returns Result<ExecutionResult, KitError>
   */
  async executeWorkflow(
    workflowId: string,
    input?: Record<string, unknown>,
    options?: ExecutionOptions
  ): Promise<Result<ExecutionResult, KitError>> {
    // Validate workflow ID
    if (!workflowId || typeof workflowId !== 'string') {
      const error = new KitError(
        'Workflow ID is required',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'workflowId', value: workflowId }
      );
      this.eventEmitter.emit('error', error);
      return err(error);
    }

    try {
      // Get SDK dependencies
      const factory = this.sdk.getFactory?.();
      if (!factory) {
        const error = new KitError(
          'SDK factory not available',
          KitErrorCode.INTERNAL_ERROR
        );
        this.eventEmitter.emit('error', error);
        return err(error);
      }

      // Create command using cached command class
      const command = new this.executeCommand(
        {
          workflowId,
          options: { input },
          ...options,
        },
        factory.getDependencies()
      );

      // Execute command
      const result = await this.sdk.executeCommand(command);

      // Convert result
      const convertResult = this.errorConverter.convertResult<any>(result);

      if (convertResult.isErr()) {
        const error = convertResult.unwrapOrElse(e => e);
        this.eventEmitter.emit('error', error);
        return err(error);
      }

      const execution = convertResult.unwrap();

      // Emit completed event
      this.eventEmitter.emit('completed', {
        executionId: execution?.executionId,
        output: execution?.output,
      });

      // Return execution result
      const startTime = execution?.startTime || 0;
      const endTime = execution?.endTime || Date.now();
      return ok({
        executionId: execution?.executionId || '',
        status: execution?.status || 'completed',
        output: execution?.output,
        duration: startTime > 0 ? endTime - startTime : 0,
      });
    } catch (error) {
      const kitError = this.errorConverter.toKitError(error);
      this.eventEmitter.emit('error', kitError);
      return err(kitError);
    }
  }

  /**
   * Register event listener
   */
  onEvent(event: string, handler: (data: any) => void): void {
    this.eventEmitter.on(event, handler);
  }
}

/**
 * Execution Builder implementation
 */
export class ExecutionBuilderImpl implements ExecutionBuilder {
  private context: ExecutionContext = {};

  constructor(
    private runner: ExecutionRunner
  ) {}

  input(data: Record<string, unknown>): ExecutionBuilder {
    this.context.input = data;
    return this;
  }

  onProgress(handler: (event: ExecutionEvent) => void): ExecutionBuilder {
    this.runner.onEvent('progress', handler);
    return this;
  }

  onError(handler: (error: Error) => void): ExecutionBuilder {
    this.runner.onEvent('error', handler);
    return this;
  }

  async execute(): Promise<Result<ExecutionResult, KitError>> {
    if (!this.context.workflowId) {
      return err(new KitError(
        'Workflow ID is required',
        KitErrorCode.VALIDATION_ERROR,
        { field: 'workflowId' }
      ));
    }

    return this.runner.executeWorkflow(
      this.context.workflowId,
      this.context.input,
      this.context.options
    );
  }

  getExecutionId(): string {
    return this.context.executionId || '';
  }

  /**
   * Set workflow ID (used internally)
   */
  setWorkflowId(id: string): ExecutionBuilder {
    this.context.workflowId = id;
    return this;
  }
}

/**
 * Execution API implementation
 */
export class ExecutionAPIImpl {
  constructor(private runner: ExecutionRunner) {}

  workflow(id: string): ExecutionBuilder {
    const builder = new ExecutionBuilderImpl(this.runner);
    builder.setWorkflowId(id);
    return builder;
  }
}
