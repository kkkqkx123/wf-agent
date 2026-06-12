/**
 * Mock LLMWrapper - Simulates LLM responses for integration testing
 *
 * Extends the real LLMWrapper to return configurable canned responses
 * without making real API calls. This allows the full coordinator chain
 * (AgentLoopCoordinator -> AgentLoopExecutor -> AgentExecutionCoordinator ->
 * AgentIterationCoordinator -> CoreLLMExecutionCoordinator -> LLMExecutor)
 * to run with realistic mock data.
 *
 * Supports:
 * - Single default response (simple mode)
 * - Sequential response sequences (for multi-iteration tests)
 * - Tool calls in responses (for tool execution tests)
 * - Error throwing (for error handling tests)
 * - Response delay control
 */

import type { LLMRequest, LLMResult, LLMToolCall, Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { LLMError } from "@wf-agent/types";
import { LLMWrapper } from "@/core/llm/wrapper.js";
import { MessageStream } from "@/core/llm/message-stream.js";
import type { EventRegistry } from "@/core/registry/event-registry.js";

// =============================================================================
// Types
// =============================================================================

export interface MockResponseEntry {
  /** Response text content */
  content: string;
  /**
   * Optional tool calls to include in the LLM result.
   * Simplified format: `{ id, name, arguments }` (no `function` wrapper needed).
   * When present, the agent loop will execute these tools and continue
   * to the next iteration, enabling multi-iteration testing.
   */
  toolCalls?: MockToolCall[];
  /** Simulated delay in ms for this response (default: 5) */
  delay?: number;
}

/**
 * Simplified tool call format for mock responses.
 * Will be converted to LLMToolCall by MockLLMWrapper.generate().
 */
export interface MockToolCall {
  id: string;
  name: string;
  arguments: string;
}

// =============================================================================
// MockLLMWrapper
// =============================================================================

export class MockLLMWrapper extends LLMWrapper {
  private responseSequence: MockResponseEntry[] = [];
  private defaultResponse = "Mock LLM response for integration testing.";
  private callCount = 0;
  private defaultDelay = 5;
  private throwOnRequest = -1;
  private throwErrorMessage = "Mock LLM error for integration testing.";

  constructor(eventManager?: EventRegistry) {
    super(eventManager);
  }

  /**
   * Set the response sequence for subsequent LLM calls.
   * Once exhausted, the last response is repeated.
   */
  setResponseSequence(sequence: MockResponseEntry[]): void {
    this.responseSequence = [...sequence];
    this.callCount = 0;
  }

  /**
   * Set a single default response for all LLM calls.
   */
  setDefaultResponse(response: string): void {
    this.defaultResponse = response;
    this.responseSequence = [];
    this.callCount = 0;
  }

  /**
   * Configure an error to be thrown on the Nth call (1-indexed).
   */
  setThrowOnRequest(requestNumber: number, message?: string): void {
    this.throwOnRequest = requestNumber;
    if (message) {
      this.throwErrorMessage = message;
    }
    this.callCount = 0;
  }

  /**
   * Clear throw-on-request configuration.
   */
  clearThrowOnRequest(): void {
    this.throwOnRequest = -1;
  }

  /**
   * Get total call count.
   */
  getCallCount(): number {
    return this.callCount;
  }

  /**
   * Non-streaming generation.
   */
  override async generate(_request: LLMRequest): Promise<Result<LLMResult, LLMError>> {
    this.callCount++;

    // Check if we should throw on this request
    if (this.throwOnRequest > 0 && this.callCount >= this.throwOnRequest) {
      return err(new LLMError(this.throwErrorMessage, "MOCK", "mock-model", undefined, undefined));
    }

    // Determine which response to use
    const idx =
      this.responseSequence.length > 0
        ? Math.min(this.callCount - 1, this.responseSequence.length - 1)
        : -1;

    let content: string;
    let toolCalls: LLMToolCall[] | undefined;
    let delay: number;

    if (idx >= 0) {
      const entry = this.responseSequence[idx]!;
      content = entry.content;
      // Convert simplified mock tool calls to proper LLMToolCall format
      toolCalls = entry.toolCalls?.map(tc => ({
        id: tc.id,
        type: "function" as const,
        function: { name: tc.name, arguments: tc.arguments },
      }));
      delay = entry.delay ?? this.defaultDelay;
    } else {
      content = this.defaultResponse;
      toolCalls = undefined;
      delay = this.defaultDelay;
    }

    if (delay > 0) {
      await new Promise(resolve => setTimeout(resolve, delay));
    }

    return ok({
      id: `mock-result-${this.callCount}`,
      model: "mock-model-v1",
      content,
      message: {
        role: "assistant",
        content,
        toolCalls,
        id: `mock-msg-${this.callCount}`,
        timestamp: Date.now(),
      },
      toolCalls,
      finishReason: toolCalls && toolCalls.length > 0 ? "tool_calls" : "stop",
      duration: delay,
    });
  }

  /**
   * Streaming generation.
   */
  override async generateStream(request: LLMRequest): Promise<Result<MessageStream, LLMError>> {
    const result = await this.generate(request);
    if (result.isErr()) {
      return err(result.error);
    }

    const stream = new MessageStream();
    const value = result.value;
    stream.pushText(value.content);
    stream.setFinalResult(value);
    stream.end();
    return ok(stream);
  }

  /**
   * Reset call count and clear recorded state.
   */
  reset(): void {
    this.callCount = 0;
    this.responseSequence = [];
    this.defaultResponse = "Mock LLM response for integration testing.";
    this.throwOnRequest = -1;
  }
}
