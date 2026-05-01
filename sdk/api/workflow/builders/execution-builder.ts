/**
 * ExecutionBuilder - Fluid Execution Builder
 * Provides a chained API to configure and execute workflows
 * Supports Result, Promise and Observable interfaces
 */

import type { WorkflowExecutionResult, WorkflowExecutionOptions, Event } from "@wf-agent/types";
import { ok, err, getErrorOrNew, withAbortSignal, now } from "@wf-agent/common-utils";
import type { Result, EventType } from "@wf-agent/types";
import { Observable, create, type Observer } from "../../shared/utils/observable.js";
import { ExecuteWorkflowCommand } from "../operations/execution/execute-workflow-command.js";
import { ExecutionError as SDKExecutionError } from "@wf-agent/types";
import { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";

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
   * 
   * This method integrates with EventRegistry to ensure events are:
   * 1. Persisted and queryable
   * 2. Available to all subscribers via EventRegistry.on()
   * 3. Forwarded to Observable subscribers for convenience
   * 
   * @returns Observable<Event>
   */
  executeAsync(): Observable<Event> {
    if (!this.workflowId) {
      return create((observer: Observer<Event>) => {
        observer.error(new Error("Workflow ID not set, please call withWorkflow() first."));
        return () => {};
      });
    }

    const workflowId = this.workflowId;
    const eventManager = this.dependencies.getEventManager();
    let executionId: string | undefined;

    return create((observer: Observer<Event>) => {
      // Creating an AbortController for canceling execution
      this.abortController = new AbortController();
      const signal = this.abortController.signal;

      // Subscribe to EventRegistry events and forward to Observable
      const eventTypes: EventType[] = [
        "WORKFLOW_EXECUTION_STARTED",
        "WORKFLOW_EXECUTION_COMPLETED",
        "WORKFLOW_EXECUTION_FAILED",
        "WORKFLOW_EXECUTION_CANCELLED",
        "NODE_COMPLETED",
      ];

      const unsubscribers: Array<() => void> = [];
      
      for (const eventType of eventTypes) {
        const unsubscribe = eventManager.on(eventType, (event: Event) => {
          // Only forward events for this execution
          if (event.executionId === executionId || !executionId) {
            observer.next(event);
          }
        });
        unsubscribers.push(unsubscribe);
      }

      // Send start event immediately (will be replaced by EventRegistry event)
      observer.next({
        id: `evt_${now()}`,
        type: "WORKFLOW_EXECUTION_STARTED",
        timestamp: now(),
        workflowId,
        executionId: "",
        metadata: {},
        input: this.options.input || {},
      } as Event);

      // Implementation workflow
      const executePromise = this.executeWithSignal(signal);

      // Listening to execution results
      executePromise.then(result => {
        if (result.isOk()) {
          executionId = result.value.executionId;
          // Note: WORKFLOW_EXECUTION_COMPLETED event will be emitted by EventRegistry
          // and forwarded via the subscription above
          observer.complete();
        } else {
          if (signal.aborted) {
            // Note: WORKFLOW_EXECUTION_CANCELLED event will be emitted by EventRegistry
            observer.complete();
          } else {
            // Note: WORKFLOW_EXECUTION_FAILED event will be emitted by EventRegistry
            observer.error(result.error);
          }
        }
      }).catch(error => {
        observer.error(error);
      });

      // Returns the unsubscribe function
      return {
        unsubscribe: () => {
          // Unsubscribe from EventRegistry
          unsubscribers.forEach(unsub => unsub());
          
          // Abort execution if still running
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
   * @returns Observable<Event>
   */
  observeProgress(): Observable<Event> {
    return create((observer: Observer<Event>) => {
      const callback = (progress: unknown) => {
        const progressRecord = progress as Record<string, unknown>;
        observer.next({
          id: `evt_${now()}`,
          type: "NODE_COMPLETED",
          timestamp: now(),
          workflowId: this.workflowId!,
          executionId: (progressRecord["executionId"] as string) || "unknown",
          nodeId: (progressRecord["currentNodeId"] as string) || "unknown",
          output: progress,
          executionTime: 0,
          metadata: {},
        } as Event);
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
   * @returns Observable<Event>
   */
  observeNodeExecuted(): Observable<Event> {
    return create((observer: Observer<Event>) => {
      const callback = (result: unknown) => {
        const resultRecord = result as Record<string, unknown>;
        observer.next({
          id: `evt_${now()}`,
          type: "NODE_COMPLETED",
          timestamp: now(),
          workflowId: this.workflowId!,
          executionId: (resultRecord["executionId"] as string) || "unknown",
          nodeId: (resultRecord["nodeId"] as string) || "unknown",
          output: result,
          executionTime: (resultRecord["executionTime"] as number) || 0,
          metadata: {},
        } as Event);
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
   * @returns Observable<Event>
   */
  observeError(): Observable<Event> {
    return create((observer: Observer<Event>) => {
      const callback = (error: unknown) => {
        const errorRecord = error as Record<string, unknown>;
        observer.next({
          id: `evt_${now()}`,
          type: "WORKFLOW_EXECUTION_FAILED",
          timestamp: now(),
          workflowId: this.workflowId!,
          executionId: (errorRecord["executionId"] as string) || "unknown",
          error: getErrorOrNew(error),
          metadata: {},
        } as Event);
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
   * @returns Observable<Event>
   */
  observeAll(): Observable<Event> {
    return create((observer: Observer<Event>) => {
      const subscriptions: Array<{ unsubscribe: () => void; closed: boolean }> = [];

      // Subscribe to progress events
      subscriptions.push(
        this.observeProgress().subscribe(
          (event: Event) => observer.next(event),
          (err: unknown) => observer.error(err),
          () => {},
        ),
      );

      // Subscribe to node execution events
      subscriptions.push(
        this.observeNodeExecuted().subscribe(
          (event: Event) => observer.next(event),
          (err: unknown) => observer.error(err),
          () => {},
        ),
      );

      // Subscribe to error events
      subscriptions.push(
        this.observeError().subscribe(
          (event: Event) => observer.next(event),
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
