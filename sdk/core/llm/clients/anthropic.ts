/**
 * Anthropic Client Implementation
 *
 * Implementations for calling the Anthropic API, handling Anthropic-specific request and response formats
 * Supports both streaming and non-streaming calls
 */

import { BaseLLMClient } from "../base-client.js";
import { AnthropicFormatter } from "../formatters/index.js";
import type { LLMProfile, LLMRequest, TokenCountResult } from "@wf-agent/types";

/**
 * Anthropic Client
 */
export class AnthropicClient extends BaseLLMClient {
  constructor(profile: LLMProfile) {
    const apiVersion = (profile.metadata?.["apiVersion"] as string) || "2023-06-01";
    super(profile, new AnthropicFormatter(apiVersion));
  }

  /**
   * Count the number of tokens
   * Call the Anthropic /v1/messages/count_tokens API
   * @param request: LLM request
   * @returns: Token count result
   */
  override async countTokens(request: LLMRequest): Promise<TokenCountResult> {
    const formatter = this.formatter as AnthropicFormatter;
    const config = this.getFormatterConfig(false);
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
