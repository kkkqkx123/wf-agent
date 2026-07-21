/**
 * AgentExecutionBuilder - Agent Execution Builder
 *
 * Provides a fluent chain-of-command API for configuring and executing
 * Agent Loop executions. Mirrors the workflow ExecutionBuilder pattern
 * for consistency across the SDK.
 *
 * Features:
 * - Fluent configuration interface (withConfig, withInput, withTimeout, etc.)
 * - Synchronous execution returning Result type
 * - Asynchronous execution returning Observable<Event> with progress tracking
 * - Execution cancellation support
 * - Event observation (progress, iteration, error)
 */

import type { AgentLoopRuntimeConfig, AgentLoopResult, BaseEvent, EventType } from "@wf-agent/types";
import { ok, err, getErrorOrNew, now } from "@wf-agent/common-utils";
import { withAbortSignal } from "../../../shared/utils/interruption/index.js";
import type { Result } from "@wf-agent/types";
import { Observable, create, type Observer } from "../../shared/utils/observable.js";
import { RunAgentLoopCommand } from "../operations/run-agent-loop-command.js";
import { ExecutionError as SDKExecutionError } from "@wf-agent/types";
import { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import { GlobalContext } from "../../../shared/global-context.js";

/**
 * AgentExecutionBuilder - Fluent builder for Agent Loop execution
 */
export class AgentExecutionBuilder {
  private agentConfig?: AgentLoopRuntimeConfig;
  private options: {
    input?: Record<string, unknown>;
    maxIterations?: number;
    timeoutMs?: number;
    onProgress?: (progress: unknown) => void;
    onIteration?: (iteration: unknown) => void;
    onError?: (error: unknown) => void;
  } = {};
  private onProgressCallbacks: Array<(progress: unknown) => void> = [];
  private onErrorCallbacks: Array<(error: unknown) => void> = [];
  private abortController?: AbortController;
  private readonly dependencies: APIDependencyManager;

  constructor(globalContext: GlobalContext) {
    this.dependencies = new APIDependencyManager(globalContext);
  }

  /**
   * Set the Agent Loop configuration
   * @param config AgentLoopRuntimeConfig
   * @returns this
   */
  withConfig(config: AgentLoopRuntimeConfig): this {
    this.agentConfig = config;
    return this;
  }

  /**
   * Set input data (initial user message)
   * @param input Input data
   * @returns this
   */
  withInput(input: Record<string, unknown>): this {
    this.options.input = input;
    return this;
  }

  /**
   * Set the maximum number of iterations
   * @param maxIterations Maximum iterations
   * @returns this
   */
  withMaxIterations(maxIterations: number): this {
    this.options.maxIterations = maxIterations;
    return this;
  }

  /**
   * Set timeout in milliseconds
   * @param timeout Timeout in milliseconds
   * @returns this
   */
  withTimeout(timeout: number): this {
    this.options.timeoutMs = timeout;
    return this;
  }

  /**
   * Set the progress callback
   * @param callback Callback function
   * @returns this
   */
  onProgress(callback: (progress: unknown) => void): this {
    this.onProgressCallbacks.push(callback);
    this.options.onProgress = callback;
    return this;
  }

  /**
   * Set the iteration callback
   * @param callback Callback function
   * @returns this
   */
  onIteration(callback: (iteration: unknown) => void): this {
    this.options.onIteration = callback;
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
   * Execute the Agent Loop (return Result type)
   * @returns Promise<Result<AgentLoopResult, Error>>
   */
  async execute(): Promise<Result<AgentLoopResult, Error>> {
    if (!this.agentConfig) {
      return err(new Error("Agent Loop config not set, please call withConfig() first."));
    }

    // Merge withConfig options into the runtime config
    const config: AgentLoopRuntimeConfig = {
      ...this.agentConfig,
    };

    if (this.options.input && this.options.input["initialUserMessage"]) {
      config.initialUserMessage = this.options.input["initialUserMessage"] as string;
    }
    if (this.options.maxIterations !== undefined) {
      config.maxIterations = this.options.maxIterations;
    }

    try {
      const command = new RunAgentLoopCommand(
        {
          config,
          timeoutMs: this.options.timeoutMs,
        },
        this.dependencies,
      );

      const executionResult = await command.execute();

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
      this.onErrorCallbacks.forEach(callback => {
        try {
          callback(error);
        } catch (callbackError) {
          throw new SDKExecutionError(
            "Error callback execution failed",
            undefined,
            "agent-execution",
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
   * Execute Agent Loop asynchronously (return Observable)
   * Provides responsive interface to support progress monitoring and cancellation
   *
   * @returns Observable<BaseEvent>
   */
  executeAsync(): Observable<BaseEvent> {
    if (!this.agentConfig) {
      return create((observer: Observer<BaseEvent>) => {
        observer.error(new Error("Agent Loop config not set, please call withConfig() first."));
        return () => {};
      });
    }

    const config = this.agentConfig;
    const eventManager = this.dependencies.getEventManager();

    return create((observer: Observer<BaseEvent>) => {
      this.abortController = new AbortController();
      const signal = this.abortController.signal;

      const eventTypes: EventType[] = [
        "AGENT_STARTED",
        "AGENT_COMPLETED",
        "AGENT_FAILED",
        "AGENT_CANCELLED",
        "AGENT_ITERATION_STARTED",
        "AGENT_ITERATION_COMPLETED",
        "AGENT_TOOL_EXECUTION_STARTED",
        "AGENT_TOOL_EXECUTION_COMPLETED",
      ];

      const unsubscribers: Array<() => void> = [];

      const unsubscribeGlobal = eventManager.onGlobal(event => {
        if (eventTypes.includes(event.type as EventType)) {
          observer.next(event as BaseEvent);
        }
      });
      unsubscribers.push(unsubscribeGlobal);

      // Send start event
      observer.next({
        id: `evt_${now()}`,
        type: "AGENT_STARTED",
        timestamp: now(),
        executionId: "",
        metadata: {},
      } as BaseEvent);

      // Build config with options
      const mergedConfig: AgentLoopRuntimeConfig = {
        ...config,
      };

      if (this.options.maxIterations !== undefined) {
        mergedConfig.maxIterations = this.options.maxIterations;
      }

      const executePromise = this.executeWithSignal(mergedConfig, signal);

      executePromise
        .then(result => {
          if (result.isOk()) {
            observer.complete();
          } else {
            if (signal.aborted) {
              observer.complete();
            } else {
              observer.error(result.error);
            }
          }
        })
        .catch(error => {
          observer.error(error);
        });

      return {
        unsubscribe: () => {
          unsubscribers.forEach(unsub => unsub());

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
   * Execute Agent Loop using AbortSignal
   * @param config AgentLoopRuntimeConfig
   * @param signal AbortSignal
   * @returns Promise<Result<AgentLoopResult, Error>>
   */
  private async executeWithSignal(
    config: AgentLoopRuntimeConfig,
    signal: AbortSignal,
  ): Promise<Result<AgentLoopResult, Error>> {
    const result = await withAbortSignal(async () => {
      const command = new RunAgentLoopCommand(
        {
          config,
          timeoutMs: this.options.timeoutMs,
        },
        this.dependencies,
      );

      const executionResult = await command.execute();

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

    if (result.ok) {
      return result.value;
    } else {
      return err(result.error);
    }
  }

  /**
   * Cancel execution
   */
  cancel(): void {
    if (this.abortController) {
      this.abortController.abort();
    }
  }

  /**
   * Get the execution progress Observable
   * @returns Observable<BaseEvent>
   */
  observeProgress(): Observable<BaseEvent> {
    return create((observer: Observer<BaseEvent>) => {
      const callback = (progress: unknown) => {
        const progressRecord = progress as Record<string, unknown>;
        observer.next({
          id: `evt_${now()}`,
          type: "AGENT_ITERATION_COMPLETED",
          timestamp: now(),
          executionId: (progressRecord["executionId"] as string) || "unknown",
          metadata: {},
        } as BaseEvent);
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
   * Get the error event Observable
   * @returns Observable<BaseEvent>
   */
  observeError(): Observable<BaseEvent> {
    return create((observer: Observer<BaseEvent>) => {
      const callback = (error: unknown) => {
        observer.next({
          id: `evt_${now()}`,
          type: "AGENT_FAILED",
          timestamp: now(),
          executionId: "unknown",
          error: getErrorOrNew(error),
          metadata: {},
        } as BaseEvent);
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
}