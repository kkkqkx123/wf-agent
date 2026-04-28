/**
 * LLM Client Base Class
 *
 * Defines the general interface and implementation for the client, providing common request handling logic.
 * Integrates with HttpClient to offer unified HTTP request processing.
 * Uses the Formatter strategy pattern to handle format conversions from different providers.
 */

import type {
  LLMClient,
  LLMRequest,
  LLMResult,
  LLMProfile,
  TokenCountResult,
  LLMUsage,
} from "@wf-agent/types";
import { HttpClient, SseTransport } from "@wf-agent/common-utils";
import { BaseFormatter, type FormatterConfig } from "./formatters/index.js";

/**
 * LLM Client Base Class
 *
 * All HTTP-based provider clients inherit from BaseLLMClient
 * Use the Formatter strategy pattern to handle format conversion
 */
export class BaseLLMClient implements LLMClient {
  protected readonly profile: LLMProfile;
  protected readonly httpClient: HttpClient;
  protected readonly formatter: BaseFormatter;

  constructor(profile: LLMProfile, formatter: BaseFormatter) {
    this.profile = profile;
    this.formatter = formatter;

    this.httpClient = new HttpClient({
      baseURL: profile.baseUrl || "",
      timeout: profile.timeout || 30000,
      maxRetries: profile.maxRetries || 3,
      retryDelay: profile.retryDelay || 1000,
      enableCircuitBreaker: true,
      enableRateLimiter: true,
    });
  }

  /**
   * Get the Formatter configuration
   */
  protected getFormatterConfig(stream: boolean = false): FormatterConfig {
    return {
      profile: this.profile,
      stream,
    };
  }

  /**
   * Non-streaming generation
   */
  async generate(request: LLMRequest): Promise<LLMResult> {
    const config = this.getFormatterConfig(false);
    const { httpRequest } = this.formatter.buildRequest(request, config);

    const response = await this.httpClient.post(httpRequest.url, httpRequest.body, {
      headers: httpRequest.headers,
      query: httpRequest.query,
    });

    return this.formatter.parseResponse(response.data, config);
  }

  /**
   * Stream generation
   */
  async *generateStream(request: LLMRequest): AsyncIterable<LLMResult> {
    const config = this.getFormatterConfig(true);
    const { httpRequest } = this.formatter.buildRequest(request, config);

    // Create an SseTransport instance
    const transport = new SseTransport(this.profile.baseUrl, httpRequest.headers);

    // Perform streaming requests using SseTransport
    const stream = transport.executeStream(httpRequest.url, {
      query: httpRequest.query,
      method: "POST",
      body: httpRequest.body,
    });

    // Accumulated token statistics
    let accumulatedUsage: LLMUsage | null = null;

    // Handling streaming responses
    for await (const line of stream) {
      const result = this.formatter.parseStreamLine(line as string, config);

      if (result.valid && result.chunk) {
        // Cumulative token statistics (accumulated, not overwritten)
        if (result.chunk.usage) {
          const currentUsage = result.chunk.usage;
          if (accumulatedUsage) {
            accumulatedUsage = {
              promptTokens: currentUsage.promptTokens ?? accumulatedUsage.promptTokens,
              completionTokens: currentUsage.completionTokens ?? accumulatedUsage.completionTokens,
              totalTokens: currentUsage.totalTokens ?? accumulatedUsage.totalTokens,
              reasoningTokens: currentUsage.reasoningTokens ?? accumulatedUsage.reasoningTokens,
            };
          } else {
            accumulatedUsage = {
              promptTokens: currentUsage.promptTokens ?? 0,
              completionTokens: currentUsage.completionTokens ?? 0,
              totalTokens: currentUsage.totalTokens ?? 0,
              reasoningTokens: currentUsage.reasoningTokens,
            };
          }
        }

        // Build an LLMResult
        const llmResult: LLMResult = {
          id: `stream-${Date.now()}`,
          model: this.profile.model,
          content: result.chunk.delta || "",
          message: {
            role: "assistant",
            content: result.chunk.delta || "",
          },
          usage:
            result.chunk.finishReason && accumulatedUsage ? accumulatedUsage : result.chunk.usage,
          finishReason: result.chunk.finishReason || "",
          duration: 0,
          metadata: {
            raw: result.chunk.raw,
          },
        };

        yield llmResult;
      }
    }
  }

  /**
   * Obtain client information
   */
  public getClientInfo(): {
    provider: string;
    model: string;
    version: string;
  } {
    return {
      provider: this.profile.provider,
      model: this.profile.model,
      version: "2.0.0",
    };
  }

  /**
   * Count the number of tokens
   * The default implementation throws an error; subclasses need to override this method.
   * @param request LLM request
   * @returns: Token count result
   */
  async countTokens(_request: LLMRequest): Promise<TokenCountResult> {
    throw new Error("countTokens is not supported by this client");
  }
}
