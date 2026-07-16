/**
 * LLM Client Implementation
 *
 * Unified client implementation for all LLM providers.
 * Uses the Formatter strategy pattern to handle format conversion for different providers.
 */

import type {
  LLMClient,
  LLMRequest,
  LLMResult,
  LLMProfile,
  TokenCountResult,
  LLMUsage,
} from "@wf-agent/types";
import { HttpClient, HttpSseTransport as SseTransport } from "../../services/index.js";
import { BaseFormatter, type FormatterConfig } from "./formatters/index.js";
import { AnthropicFormatter } from "./formatters/anthropic.js";

/**
 * LLM Client Implementation
 *
 * All HTTP-based provider clients use this unified implementation.
 * Uses the Formatter strategy pattern to handle format conversion.
 */
export class LLMClientImpl implements LLMClient {
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
   * Merges request-level toolCallFormat over profile-level configuration.
   */
  protected getFormatterConfig(request: LLMRequest, stream: boolean = false): FormatterConfig {
    return {
      profile: this.profile,
      stream,
      toolCallFormat: request.toolCallFormat ?? this.profile.toolCallFormat,
    };
  }

  /**
   * Non-streaming generation
   */
  async generate(request: LLMRequest): Promise<LLMResult> {
    const config = this.getFormatterConfig(request, false);
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
    const config = this.getFormatterConfig(request, true);
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
   *
   * Only Anthropic supports token counting.
   * Delegates to the formatter if it implements buildCountTokensRequest.
   *
   * @param request LLM request
   * @returns Token count result
   */
  async countTokens(request: LLMRequest): Promise<TokenCountResult> {
    if (!(this.formatter instanceof AnthropicFormatter)) {
      throw new Error("countTokens is not supported by this client");
    }

    const formatter = this.formatter as AnthropicFormatter;
    const config = this.getFormatterConfig(request, false);
    const { httpRequest } = formatter.buildCountTokensRequest(request, config);

    const response = await this.httpClient.post<Record<string, unknown>>(
      httpRequest.url,
      httpRequest.body,
      {
        headers: httpRequest.headers,
        query: httpRequest.query,
      },
    );

    return {
      inputTokens: ((response.data as Record<string, unknown>)["input_tokens"] as number) || 0,
      raw: response.data,
    };
  }
}
