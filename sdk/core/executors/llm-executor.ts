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

import type { LLMMessage, LLMResult, ToolSchema } from "@wf-agent/types";
import { LLMWrapper } from "../llm/wrapper.js";
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
 * LLM Executor Class (Stateless)
 *
 * Provides methods to execute LLM calls without holding any state. All state is passed in as parameters, and results are returned as values. The lifecycle is managed by the DI (Dependency Injection) container.
 *
 *
 */
export class LLMExecutor {
  constructor(private llmWrapper: LLMWrapper) {}

  /**
   * Execute a single LLM call
   *
   * Note: This method only performs one LLM call and does not handle loops of tool calls.
   * The coordination of tool calls is the responsibility of the LLMCoordinator.
   *
   * Note: This method does NOT handle interruptions.
   * Interruption handling is the responsibility of the caller (Coordinator).
   *
   * @param messages Array of messages
   * @param requestData Request data
   * @param options Execution options (including AbortSignal and context information)
   * @returns LLM execution result
   * @throws LLMError on failure (caller should handle interruptions)
   */
  async executeLLMCall(
    messages: LLMMessage[],
    requestData: LLMExecutionRequestData,
    options?: { abortSignal?: AbortSignal; executionId?: string; nodeId?: string },
  ): Promise<LLMExecutionResult> {
    logger.debug("LLM call started", {
      profileId: requestData.profileId,
      executionId: options?.executionId,
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

    // Perform an LLM call.
    let finalResult: LLMResult;
    if (llmRequest.stream) {
      // Stream-based invocation - Returns Result<MessageStream, LLMError>
      const streamResult = await this.llmWrapper.generateStream(llmRequest);

      if (streamResult.isErr()) {
        logger.warn("LLM stream call failed", {
          profileId: requestData.profileId,
          error: streamResult.error.message,
        });
        // Throw error directly, let caller handle interruptions
        throw streamResult.error;
      }

      const messageStream = streamResult.value;

      // Link external abort signal to MessageStream for fine-grained interruption
      if (options?.abortSignal) {
        messageStream.setAbortSignal(options.abortSignal);
      }

      // Note: MessageStream events can be listened to by callers if needed.
      // For better observability, callers can attach listeners before awaiting done():
      // ```typescript
      // messageStream.on("abort", (event) => {
      //   // Handle abort event
      // });
      // await messageStream.done();
      // ```

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
        // Throw error directly, let caller handle interruptions
        throw result.error;
      }

      finalResult = result.value;
    }

    logger.debug("LLM call completed", {
      profileId: requestData.profileId,
      finishReason: finalResult.finishReason,
      hasToolCalls: !!finalResult.toolCalls && finalResult.toolCalls.length > 0,
      toolCallCount: finalResult.toolCalls?.length,
    });

    // Build and return result
    return {
      content: finalResult.content,
      usage: finalResult.usage,
      finishReason: finalResult.finishReason,
      toolCalls: finalResult.toolCalls?.map(tc => ({
        id: tc.id,
        name: tc.function.name,
        arguments: tc.function.arguments,
      })),
    };
  }
}
