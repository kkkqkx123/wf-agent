/**
 * ExecutionBuilder - Fluid Execution Builder
 * Provides a chained API to configure and execute workflows
 * Supports Result, Promise and Observable interfaces
 */

import type { WorkflowExecutionResult, WorkflowExecutionOptions } from "@wf-agent/types";
import { ok, err, getErrorOrNew, withAbortSignal, now } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { Observable, create, type Observer } from "../../shared/utils/observable.js";
import { ExecuteWorkflowCommand } from "../operations/execution/execute-workflow-command.js";
import { ExecutionError as SDKExecutionError } from "@wf-agent/types";
import { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type {
  ExecutionEvent,
  StartEvent,
  CompleteEvent,
  ErrorEvent,
  CancelledEvent,
  ProgressEvent,
  NodeExecutedEvent,
} from "../../shared/types/execution-events.js";

/**
 * ExecutionBuilder - Fluid Execution Builder
 */
export class ExecutionBuilder {
  private workflowId?: string;
  private options: WorkflowExecutionOptions = {};
  private onProgressCallbacks: Array<(progress: unknown) => void> = [];
  private onErrorCallbacks: Array<(error: unknown) => void> = [];
  private abortController?: AbortController;
  private readonly dependencies: APIDependencyManager;

  constructor() {
    // Instead of relying on the WorkflowExecutorAPI, use the Command mode
    this.dependencies = new APIDependencyManager();
  }

  /**
   * Setting the workflow ID
   * @param workflowId The workflow ID.
   * @returns this
   */
  withWorkflow(workflowId: string): this {
    this.workflowId = workflowId;
    return this;
  }

  /**
   * Setting Input Data
   * @param input Input data
   * @returns this
   */
  withInput(input: Record<string, unknown>): this {
    this.options.input = input;
    return this;
  }

  /**
   * Setting the maximum number of execution steps
   * @param maxSteps maxSteps
   * @returns this
   */
  withMaxSteps(maxSteps: number): this {
    this.options.maxSteps = maxSteps;
    return this;
  }

  /**
   * Set timeout in milliseconds
   * @param timeout
   * @returns this
   */
  withTimeout(timeout: number): this {
    this.options.timeout = timeout;
    return this;
  }

  /**
   * Enable checkpoints
   * @param enable Whether to enable
   * @returns this
   */
  withCheckpoints(enable: boolean = true): this {
    this.options.enableCheckpoints = enable;
    return this;
  }

  /**
   * Setting the node execution callback
   * @param callback Callback function
   * @returns this
   */
  onNodeExecuted(callback: (result: unknown) => void | Promise<void>): this {
    this.options.onNodeExecuted = callback;
    return this;
  }

  /**
   * Setting the progress callback
   * @param callback Callback function
   * @returns this
   */
  onProgress(callback: (progress: unknown) => void): this {
    this.onProgressCallbacks.push(callback);
    return this;
  }

  /**
   * Set an error callback
   * @param callback The callback function
   * @returns this
   */
  onError(callback: (error: unknown) => void): this {
    this.onErrorCallbacks.push(callback);
    this.options.onError = callback;
    return this;
  }

  /**
   * Execute the workflow (return Result type)
   * @returns Promise<Result<WorkflowExecutionResult, Error>>
   */
  async execute(): Promise<Result<WorkflowExecutionResult, Error>> {
    if (!this.workflowId) {
      return err(new Error("Workflow ID not set, please call withWorkflow() first."));
    }

    try {
      // Workflow execution using Command mode
      const command = new ExecuteWorkflowCommand(
        {
          workflowId: this.workflowId,
          options: this.options,
        },
        this.dependencies,
      );

      const executionResult = await command.execute();

      // Handling the ExecutionResult type
      if (executionResult.result.isOk()) {
        return ok(executionResult.result.unwrap());
      } else {
        return err(
          new Error(
            executionResult.result.unwrapOrElse((e: Error) => e.message) || "Execution failed.",
          ),
        );
      }
    } catch (error) {
      // trigger an error callback
      this.onErrorCallbacks.forEach(callback => {
        try {
          callback(error);
        } catch (callbackError) {
          // Throw an error and let the caller decide what to do with it
          throw new SDKExecutionError(
            "Error callback execution failed",
            undefined,
            this.workflowId,
            {
              operation: "error_callback",
            },
            getErrorOrNew(callbackError),
          );
        }
      });

      return err(getErrorOrNew(error));
    }
  }

  /**
   * Execute workflows asynchronously (return Observable)
   * Provides responsive interface to support progress monitoring and cancelation
   * @returns Observable<ExecutionEvent>
   */
  executeAsync(): Observable<ExecutionEvent> {
    if (!this.workflowId) {
      return create((observer: Observer<ExecutionEvent>) => {
        observer.error(new Error("Workflow ID not set, please call withWorkflow() first."));
        return () => {};
      });
    }

    const workflowId = this.workflowId; // The workflowId has been determined to exist here
    let threadId: string | undefined;

    return create((observer: Observer<ExecutionEvent>) => {
      // Creating an AbortController for canceling execution
      this.abortController = new AbortController();
      const signal = this.abortController.signal;

      // Send start event
      observer.next({
        type: "start",
        timestamp: now(),
        workflowId,
      } as StartEvent);

      // Implementation workflow
      const executePromise = this.executeWithSignal(signal);

      // Listening to execution results
      let executionId: string | undefined;
      executePromise.then(result => {
        if (result.isOk()) {
          executionId = result.value.executionId;
          // Send Completion Event
          observer.next({
            type: "complete",
            timestamp: now(),
            workflowId,
            executionId: executionId,
            threadId: executionId, // Backward compatibility
            result: result.value,
            executionStats: {
              duration: result.value.executionTime,
              steps: result.value.nodeResults.length,
              nodesExecuted: result.value.nodeResults.length,
            },
          } as CompleteEvent);
          observer.complete();
        } else {
          if (signal.aborted) {
            // Send Cancel Event
            observer.next({
              type: "cancelled",
              timestamp: now(),
              workflowId,
              executionId: threadId || "unknown",
              threadId: threadId || "unknown",
              reason: result.error.message,
            } as CancelledEvent);
            observer.complete();
          } else {
            // Send error event
            observer.next({
              type: "error",
              timestamp: now(),
              workflowId,
              executionId: threadId || "unknown",
              threadId: threadId || "unknown",
              error: result.error,
            } as ErrorEvent);
            observer.error(result.error);
          }
        }
      });

      // Returns the unsubscribe function
      return {
        unsubscribe: () => {
          if (this.abortController && !signal.aborted) {
            this.abortController.abort();
          }
        },
        get closed() {
          return signal.aborted;
        },
      };
    });
  }

  /**
   * Execute a workflow using AbortSignal
   * @param signal AbortSignal
   * @returns Promise<Result<WorkflowExecutionResult, Error>>
   */
  private async executeWithSignal(signal: AbortSignal): Promise<Result<WorkflowExecutionResult, Error>> {
    // Wrapping execution logic with withAbortSignal
    return withAbortSignal(async () => {
      // Workflow execution using Command mode
      const command = new ExecuteWorkflowCommand(
        {
          workflowId: this.workflowId!,
          options: this.options,
        },
        this.dependencies,
      );

      const executionResult = await command.execute();

      // Handling the ExecutionResult type
      if (executionResult.result.isOk()) {
        return ok(executionResult.result.unwrap());
      } else {
        return err(
          new Error(
            executionResult.result.unwrapOrElse((e: Error) => e.message) || "Execution failed.",
          ),
        );
      }
    }, signal);
  }

  /**
   * Cancel execution
   * @returns void
   */
  cancel(): void {
    if (this.abortController) {
      this.abortController.abort();
    }
  }

  /**
   * Get the execution progress Observable
   * @returns Observable<ProgressEvent>
   */
  observeProgress(): Observable<ProgressEvent> {
    return create((observer: Observer<ProgressEvent>) => {
      const callback = (progress: unknown) => {
        const progressRecord = progress as Record<string, unknown>;
        observer.next({
          type: "progress",
          timestamp: now(),
          workflowId: this.workflowId!,
          threadId: (progressRecord["threadId"] as string) || "unknown",
          progress: {
            status: (progressRecord["status"] as string) || "running",
            currentStep: (progressRecord["currentStep"] as number) || 0,
            totalSteps: progressRecord["totalSteps"] as number | undefined,
            currentNodeId: (progressRecord["currentNodeId"] as string) || "unknown",
            currentNodeType: (progressRecord["currentNodeType"] as string) || "unknown",
          },
        } as ProgressEvent);
      };

      this.onProgressCallbacks.push(callback);

      let unsubscribed = false;
      return {
        unsubscribe: () => {
          if (!unsubscribed) {
            unsubscribed = true;
            const index = this.onProgressCallbacks.indexOf(callback);
            if (index > -1) {
              this.onProgressCallbacks.splice(index, 1);
            }
          }
        },
        get closed() {
          return unsubscribed;
        },
      };
    });
  }

  /**
   * Get the Observable for node execution events
   * @returns Observable<NodeExecutedEvent>
   */
  observeNodeExecuted(): Observable<NodeExecutedEvent> {
    return create((observer: Observer<NodeExecutedEvent>) => {
      const callback = (result: unknown) => {
        const resultRecord = result as Record<string, unknown>;
        observer.next({
          type: "nodeExecuted",
          timestamp: now(),
          workflowId: this.workflowId!,
          threadId: (resultRecord["threadId"] as string) || "unknown",
          nodeId: (resultRecord["nodeId"] as string) || "unknown",
          nodeType: (resultRecord["nodeType"] as string) || "unknown",
          nodeResult: result,
          executionTime: (resultRecord["executionTime"] as number) || 0,
        } as NodeExecutedEvent);
      };

      this.options.onNodeExecuted = callback;

      let unsubscribed = false;
      return {
        unsubscribe: () => {
          if (!unsubscribed) {
            unsubscribed = true;
            if (this.options.onNodeExecuted === callback) {
              delete this.options.onNodeExecuted;
            }
          }
        },
        get closed() {
          return unsubscribed;
        },
      };
    });
  }

  /**
   * Get the error event Observable
   * @returns Observable<ErrorEvent>
   */
  observeError(): Observable<ErrorEvent> {
    return create((observer: Observer<ErrorEvent>) => {
      const callback = (error: unknown) => {
        const errorRecord = error as Record<string, unknown>;
        observer.next({
          type: "error",
          timestamp: now(),
          workflowId: this.workflowId!,
          threadId: (errorRecord["threadId"] as string) || "unknown",
          error: getErrorOrNew(error),
        } as ErrorEvent);
      };

      this.onErrorCallbacks.push(callback);

      let unsubscribed = false;
      return {
        unsubscribe: () => {
          if (!unsubscribed) {
            unsubscribed = true;
            const index = this.onErrorCallbacks.indexOf(callback);
            if (index > -1) {
              this.onErrorCallbacks.splice(index, 1);
            }
          }
        },
        get closed() {
          return unsubscribed;
        },
      };
    });
  }

  /**
   * Get all execution event Observables
   * @returns Observable<ExecutionEvent>
   */
  observeAll(): Observable<ExecutionEvent> {
    return create((observer: Observer<ExecutionEvent>) => {
      const subscriptions: Array<{ unsubscribe: () => void; closed: boolean }> = [];

      // Subscribe to progress events
      subscriptions.push(
        this.observeProgress().subscribe(
          (event: ProgressEvent) => observer.next(event),
          (err: unknown) => observer.error(err),
          () => {},
        ),
      );

      // Subscribe to node execution events
      subscriptions.push(
        this.observeNodeExecuted().subscribe(
          (event: NodeExecutedEvent) => observer.next(event),
          (err: unknown) => observer.error(err),
          () => {},
        ),
      );

      // Subscribe to error events
      subscriptions.push(
        this.observeError().subscribe(
          (event: ErrorEvent) => observer.next(event),
          (err: unknown) => observer.error(err),
          () => {},
        ),
      );

      return {
        unsubscribe: () => {
          subscriptions.forEach(sub => sub.unsubscribe());
        },
        get closed() {
          return subscriptions.every(sub => sub.closed);
        },
      };
    });
  }
}
