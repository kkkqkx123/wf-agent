/**
 * LLM Executor
 * Provides a stateless method for calling LLMs (Large Language Models)
 *
 * Core Responsibilities:
 * 1. Executes LLM calls (both non-streaming and streaming)
 * 2. Delegates tasks to the LLMWrapper module within the sdk/core/llm package
 *
 * Design Principles:
 * - Stateless design: Does not retain any internal state
 * - All state is passed as parameters
 * - Lifecycle is managed by the Dependency Injection (DI) container
 * - Called by the LLMCoordinator
 * - Does not handle tool calls; tool calls are coordinated by the LLMCoordinator
 *
 * Location Description:
 * - Located in the sdk/core/execution/executors directory, serving as a generic executor
 * - Can be reused by modules such as Graph and Agent
 * - Does not depend on the implementation of any specific module
 */

import { isAbortError, checkInterruption } from "@wf-agent/common-utils";
import type { InterruptionCheckResult } from "@wf-agent/common-utils";
import type { LLMMessage, LLMResult, ToolSchema } from "@wf-agent/types";
import { LLMWrapper } from "../llm/wrapper.js";
import { ExecutionError, LLMError } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "LLMExecutor" });

/**
 * LLM executes the request data.
 */
export interface LLMExecutionRequestData {
  prompt: string;
  profileId: string;
  parameters: Record<string, unknown>;
  tools?: ToolSchema[];
  /** Dynamic Tool Configuration */
  dynamicTools?: {
    /** The tool ID or name to be added dynamically */
    toolIds: string[];
    /** Tool Description Template (optional) */
    descriptionTemplate?: string;
  };
  /** The maximum number of tool calls returned per LLM invocation (default is 3) */
  maxToolCallsPerRequest?: number;
  stream?: boolean;
}

/**
 * LLM execution results
 */
export interface LLMExecutionResult {
  content: string;
  usage?: unknown;
  finishReason?: string;
  toolCalls?: Array<{
    id: string;
    name: string;
    arguments: string;
  }>;
}

/**
 * LLM execution results (including interrupt status)
 */
export type LLMExecutionResultWithInterruption =
  | { success: true; result: LLMExecutionResult }
  | { success: false; interruption: InterruptionCheckResult };

/**
 * LLM Executor Class (Stateless)
 *
 * Provides methods to execute LLM calls without holding any state. All state is passed in as parameters, and results are returned as values. The lifecycle is managed by the DI (Dependency Injection) container.
 *
 *
 */
export class LLMExecutor {
  constructor(private llmWrapper: LLMWrapper) {}

  /**
   * Handle LLM call errors
   *
   * Unify the error handling logic for both streaming and non-streaming calls
   *
   * @param error Error object
   * @param profileId LLM profile ID
   * @param options Execution options
   * @returns Execution result (including interruption status or error)
   */
  private handleLLMError(
    error: LLMError,
    profileId: string,
    options?: { abortSignal?: AbortSignal; threadId?: string; nodeId?: string },
  ): LLMExecutionResultWithInterruption {
    // Check if it is an AbortError.
    if (isAbortError(error)) {
      const result = checkInterruption(options?.abortSignal);
      // PAUSE/STOP returns the interrupted state.
      if (result.type === "paused" || result.type === "stopped") {
        return {
          success: false,
          interruption: result,
        };
      }
      // An ordinary abort (aborted) also returns an interrupted status.
      if (result.type === "aborted") {
        return {
          success: false,
          interruption: result,
        };
      }
      // Continue without terminating; throw the original error.
      throw error;
    }

    // Convert to ExecutionError and throw it.
    throw new ExecutionError(
      `LLM call failed: ${error.message}`,
      options?.nodeId,
      undefined,
      { originalError: error, profileId },
      error,
    );
  }

  /**
   * Execute a single LLM call
   *
   * Note: This method only performs one LLM call and does not handle loops of tool calls.
   * The coordination of tool calls is the responsibility of the LLMCoordinator.
   *
   * @param messages Array of messages
   * @param requestData Request data
   * @param options Execution options (including AbortSignal and context information)
   * @returns LLM execution result or interruption status
   */
  async executeLLMCall(
    messages: LLMMessage[],
    requestData: LLMExecutionRequestData,
    options?: { abortSignal?: AbortSignal; threadId?: string; nodeId?: string },
  ): Promise<LLMExecutionResultWithInterruption> {
    logger.debug("LLM call started", {
      profileId: requestData.profileId,
      threadId: options?.threadId,
      nodeId: options?.nodeId,
      messageCount: messages.length,
      stream: requestData.stream,
      hasTools: !!requestData.tools && requestData.tools.length > 0,
    });

    // Construct an LLM request
    const llmRequest = {
      profileId: requestData.profileId,
      messages: messages,
      tools: requestData.tools,
      parameters: requestData.parameters,
      stream: requestData.stream || false,
      signal: options?.abortSignal, // Pass the AbortSignal
    };

    let finalResult: LLMResult | null = null;

    // Perform an LLM call.
    if (llmRequest.stream) {
      // Stream-based invocation - Returns Result<MessageStream, LLMError>
      const streamResult = await this.llmWrapper.generateStream(llmRequest);

      if (streamResult.isErr()) {
        logger.warn("LLM stream call failed", {
          profileId: requestData.profileId,
          error: streamResult.error.message,
        });
        return this.handleLLMError(streamResult.error, requestData.profileId, options);
      }

      const messageStream = streamResult.value;

      // Wait for the stream to complete
      // The stream processes events internally via event listeners
      await messageStream.done();

      // Get the final result after stream completion
      finalResult = await messageStream.getFinalResult();
    } else {
      // Non-streaming call - Returns Result<LLMResult, LLMError>
      const result = await this.llmWrapper.generate(llmRequest);

      if (result.isErr()) {
        logger.warn("LLM call failed", {
          profileId: requestData.profileId,
          error: result.error.message,
        });
        return this.handleLLMError(result.error, requestData.profileId, options);
      }

      finalResult = result.value;
    }

    // Check results
    if (!finalResult) {
      throw new ExecutionError("No LLM result generated", undefined, undefined, {
        profileId: requestData.profileId,
      });
    }

    logger.debug("LLM call completed", {
      profileId: requestData.profileId,
      finishReason: finalResult.finishReason,
      hasToolCalls: !!finalResult.toolCalls && finalResult.toolCalls.length > 0,
      toolCallCount: finalResult.toolCalls?.length,
    });

    // Build the return result
    return {
      success: true,
      result: {
        content: finalResult.content,
        usage: finalResult.usage,
        finishReason: finalResult.finishReason,
        toolCalls: finalResult.toolCalls?.map(tc => ({
          id: tc.id,
          name: tc.function.name,
          arguments: tc.function.arguments,
        })),
      },
    };
  }
}
