/**
 * Human Relay Client
 * A client implementation that uses human input instead of LLM API calls
 *
 * Responsibilities:
 * - Implements LLMClient interface to allow Human Relay to be used like a regular LLM
 * - Converts LLMRequest to HumanRelayRequest
 * - Calls HumanRelayHandler to obtain human input
 * - Converts human input to LLMResult
 *
 * Design Principles:
 * - Stateless design
 * - Implements standard LLMClient interface for seamless integration
 * - Supports timeout and cancellation
 */

import type {
  LLMClient,
  LLMRequest,
  LLMProfile,
  HumanRelayHandler,
  HumanRelayRequest,
  HumanRelayResponse,
  HumanRelayContext,
  LLMResult,
} from "@wf-agent/types";
import { ConfigurationError, ExecutionError } from "@wf-agent/types";
import { generateId, now } from "@wf-agent/common-utils";

export interface HumanRelayClientConfig {
  /** Human Relay Handler */
  handler: HumanRelayHandler;
  /** Default timeout in milliseconds */
  defaultTimeout?: number;
  /** Context provider function that returns HumanRelayContext */
  contextProvider: () => HumanRelayContext;
}

/**
 * Human Relay Client Implementation
 *
 * Note:
 * Human Relay replaces LLM API calls with human input, allowing manual intervention
 * in the LLM conversation process. This is different from tool approval.
 */
export class HumanRelayClient implements LLMClient {
  private profile: LLMProfile;
  private handler: HumanRelayHandler;
  private defaultTimeout: number;
  private contextProvider: () => HumanRelayContext;

  constructor(profile: LLMProfile, config: HumanRelayClientConfig) {
    this.profile = profile;
    this.handler = config.handler;
    this.defaultTimeout = config.defaultTimeout || 300000; // Default 5 minutes
    this.contextProvider = config.contextProvider;

    this.validateConfig();
  }

  /**
   * Validate configuration
   */
  private validateConfig(): void {
    if (!this.handler) {
      throw new ConfigurationError("HumanRelayHandler is required", "handler", {
        code: "HANDLER_REQUIRED",
      });
    }
    if (!this.contextProvider) {
      throw new ConfigurationError("Context provider is required", "contextProvider", {
        code: "CONTEXT_PROVIDER_REQUIRED",
      });
    }
  }

  /**
   * Generate non-streaming response
   *
   * @param request LLM request
   * @returns LLM result
   */
  async generate(request: LLMRequest): Promise<LLMResult> {
    const startTime = now();
    const requestId = generateId();

    try {
      // 1. Build HumanRelayRequest
      const humanRelayRequest = this.buildHumanRelayRequest(request, requestId);

      // 2. Get HumanRelayContext from provider
      const context = this.contextProvider();

      // 3. Call handler to get human input
      const response = await this.handler.handle(humanRelayRequest, context);

      // 4. Convert to LLMResult
      return this.buildLLMResult(response, request, startTime);
    } catch (error) {
      throw new ExecutionError(
        `HumanRelay execution failed: ${error instanceof Error ? error.message : String(error)}`,
        requestId,
      );
    }
  }

  /**
   * Generate streaming response
   *
   * Note: Human Relay is essentially synchronous human input, but for interface consistency,
   * we simulate a streaming response (return all content at once)
   *
   * @param request LLM request
   * @returns Streaming response iterator
   */
  async *generateStream(request: LLMRequest): AsyncIterable<LLMResult> {
    const startTime = now();
    const requestId = generateId();

    try {
      // 1. Build HumanRelayRequest
      const humanRelayRequest = this.buildHumanRelayRequest(request, requestId);

      // 2. Get HumanRelayContext from provider
      const context = this.contextProvider();

      // 3. Call handler to get human input
      const response = await this.handler.handle(humanRelayRequest, context);

      // 4. Yield partial result (simulating streaming)
      const partialResult: LLMResult = {
        id: response.requestId,
        model: this.profile.model,
        content: response.content,
        message: {
          role: "assistant",
          content: response.content,
        },
        finishReason: "null", // Not finished yet
        duration: now() - startTime,
        metadata: {
          source: "human-relay",
          profileId: this.profile.id,
          streaming: true,
        },
      };
      yield partialResult;

      // 5. Yield final result
      const finalResult = this.buildLLMResult(response, request, startTime);
      yield finalResult;
    } catch (error) {
      throw new ExecutionError(
        `HumanRelay streaming execution failed: ${error instanceof Error ? error.message : String(error)}`,
        requestId,
      );
    }
  }

  /**
   * Build HumanRelayRequest
   */
  private buildHumanRelayRequest(request: LLMRequest, requestId: string): HumanRelayRequest {
    // Extract the last user message as prompt
    const messages = request.messages || [];
    let lastUserMessage: { role: string; content: unknown } | undefined;
    for (let i = messages.length - 1; i >= 0; i--) {
      if (messages[i]?.role === "user") {
        lastUserMessage = messages[i];
        break;
      }
    }
    const prompt = lastUserMessage?.content || "Please provide your input:";

    return {
      requestId,
      messages: request.messages || [],
      prompt: typeof prompt === "string" ? prompt : JSON.stringify(prompt),
      timeout: this.defaultTimeout,
      metadata: {
        profileId: this.profile.id,
        model: this.profile.model,
        temperature: request.parameters?.["temperature"],
        maxTokens: request.parameters?.["maxTokens"],
      },
    };
  }

  /**
   * Build LLMResult
   */
  private buildLLMResult(
    humanResponse: HumanRelayResponse,
    request: LLMRequest,
    startTime: number,
  ): LLMResult {
    return {
      id: humanResponse.requestId,
      model: this.profile.model,
      content: humanResponse.content,
      message: {
        role: "assistant",
        content: humanResponse.content,
      },
      finishReason: "stop",
      duration: now() - startTime,
      metadata: {
        source: "human-relay",
        profileId: this.profile.id,
      },
    };
  }
}
