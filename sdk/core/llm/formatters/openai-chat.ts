/**
 * OpenAI Chat Format Converter
 *
 * Implement the conversion of request and response formats for the OpenAI Chat API
 * Use the /chat/completions endpoint
 */

import { BaseFormatter } from "./base.js";
import type { LLMRequest, LLMResult, LLMMessage, LLMToolCall } from "@wf-agent/types";
import type { ToolSchema } from "@wf-agent/types";
import type { FormatterConfig, BuildRequestResult, ParseStreamChunkResult } from "./types.js";
import { convertToolsToOpenAIFormat } from "../utils/index.js";

/**
 * OpenAI Chat Format Converter
 */
export class OpenAIChatFormatter extends BaseFormatter {
  getSupportedProvider(): string {
    return "OPENAI_CHAT";
  }

  buildRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult {
    const body = this.buildRequestBody(request, config);

    // Constructing request headers
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      ...config.profile.headers,
    };

    // Add authentication headers (supporting both bearer and native methods)
    if (config.profile.apiKey) {
      Object.assign(headers, this.buildAuthHeader(config.profile.apiKey, config, "Authorization"));
    }

    // Add custom request headers
    Object.assign(headers, this.buildCustomHeaders(config));

    // Apply a custom request body.
    const finalBody = this.applyCustomBody(body, config);

    // Construct query parameters
    const queryString = this.buildQueryString(config);

    return {
      httpRequest: {
        url: `/chat/completions${queryString}`,
        method: "POST",
        headers,
        body: finalBody,
        timeout: config.timeout,
      },
      transformedBody: finalBody,
    };
  }

  parseResponse(data: unknown): LLMResult {
    const dataRecord = data as Record<string, unknown>;
    const choices = dataRecord["choices"] as Array<Record<string, unknown>> | undefined;
    const choice = choices?.[0];
    const message = choice?.["message"] as Record<string, unknown> | undefined;

    // Extract reasoning content (for DeepSeek R1, o1, etc.)
    const reasoningContent = message?.["reasoning_content"] as string | undefined;
    // Extract reasoning tokens from usage details
    const usage = dataRecord["usage"] as Record<string, unknown> | undefined;
    const completionTokensDetails = usage?.["completion_tokens_details"] as
      | Record<string, number>
      | undefined;
    const reasoningTokens = completionTokensDetails?.["reasoning_tokens"] || 0;

    return {
      id: dataRecord["id"] as string,
      model: dataRecord["model"] as string,
      content: (message?.["content"] as string) || "",
      message: this.parseMessage(message),
      toolCalls: message?.["tool_calls"]
        ? this.parseToolCalls(message["tool_calls"] as unknown[])
        : undefined,
      usage: usage
        ? {
            promptTokens: usage["prompt_tokens"] as number,
            completionTokens: usage["completion_tokens"] as number,
            totalTokens: usage["total_tokens"] as number,
            // Add the `reasoningTokens` field to support the statistics of the thinking model.
            reasoningTokens: reasoningTokens > 0 ? reasoningTokens : undefined,
          }
        : undefined,
      finishReason: choice?.["finish_reason"] as string,
      duration: 0,
      metadata: {
        created: dataRecord["created"] as number,
        systemFingerprint: dataRecord["system_fingerprint"] as string,
      },
      reasoningContent,
      reasoningTokens: reasoningTokens > 0 ? reasoningTokens : undefined,
    };
  }

  parseStreamChunk(data: unknown): ParseStreamChunkResult {
    const dataRecord = data as Record<string, unknown>;
    const choices = dataRecord["choices"] as Array<Record<string, unknown>> | undefined;
    const choice = choices?.[0];
    if (!choice) {
      return { chunk: { done: false }, valid: false };
    }

    const delta = choice["delta"] as Record<string, unknown> | undefined;

    // Extract reasoning content delta (for DeepSeek R1, o1, etc.)
    const reasoningDelta = delta?.["reasoning_content"] as string | undefined;

    // Extract tool calls delta from streaming chunks
    const toolCallsDelta = delta?.["tool_calls"]
      ? this.parseToolCalls(delta["tool_calls"] as unknown[])
      : undefined;

    // Extract reasoning tokens from usage details
    const usage = dataRecord["usage"] as Record<string, unknown> | undefined;
    const completionTokensDetails = usage?.["completion_tokens_details"] as
      | Record<string, number>
      | undefined;
    const reasoningTokens = completionTokensDetails?.["reasoning_tokens"] || 0;

    // Handle finish_reason: stop, tool_calls, length, content_filter all indicate completion
    const finishReason = choice["finish_reason"] as string;
    const isDone = ["stop", "tool_calls", "length", "content_filter"].includes(finishReason);

    return {
      chunk: {
        delta: (delta?.["content"] as string) || "",
        done: isDone,
        usage: usage
          ? {
              promptTokens: usage["prompt_tokens"] as number,
              completionTokens: usage["completion_tokens"] as number,
              totalTokens: usage["total_tokens"] as number,
              // Add the `reasoningTokens` field to support the statistics of the thinking model.
              reasoningTokens: reasoningTokens > 0 ? reasoningTokens : undefined,
            }
          : undefined,
        finishReason,
        modelVersion: dataRecord["model"] as string,
        raw: data,
        reasoningDelta,
        toolCallsDelta,
      },
      valid: true,
    };
  }

  convertTools(tools: ToolSchema[]): unknown {
    return convertToolsToOpenAIFormat(tools);
  }

  convertMessages(messages: LLMMessage[]): unknown[] {
    return messages.map(msg => {
      const converted: {
        role: string;
        content: unknown;
        tool_calls?: unknown[];
        name?: string;
        tool_call_id?: string;
      } = {
        role: msg.role,
        content: msg.content,
      };

      // Add tool calls
      if (msg.toolCalls && msg.toolCalls.length > 0) {
        converted.tool_calls = msg.toolCalls.map(call => ({
          id: call.id,
          type: call.type,
          function: {
            name: call.function.name,
            arguments: call.function.arguments,
          },
        }));
      }

      // Add tool call ID
      if (msg.toolCallId) {
        converted.tool_call_id = msg.toolCallId;
      }

      return converted;
    });
  }

  parseToolCalls(toolCalls: unknown): LLMToolCall[] {
    if (!Array.isArray(toolCalls)) return [];
    return toolCalls.map(call => ({
      id: (call as { id: string }).id,
      type: ((call as { type?: string }).type || "function") as "function",
      function: {
        name: (call as { function: { name: string } }).function.name,
        arguments:
          typeof (call as { function: { arguments: string | unknown } }).function.arguments ===
          "string"
            ? (call as { function: { arguments: string } }).function.arguments
            : JSON.stringify((call as { function: { arguments: unknown } }).function.arguments),
      },
    }));
  }

  /**
   * Construct the request body
   */
  private buildRequestBody(request: LLMRequest, config: FormatterConfig): Record<string, unknown> {
    const parameters = this.mergeParameters(config.profile.parameters, request.parameters);

    const body: Record<string, unknown> = {
      model: config.profile.model,
      messages: this.convertMessages(request.messages),
      stream: config.stream || false,
    };

    // Merge parameters (excluding stream, which has been processed separately)
    const { stream: _stream, ...otherParams } = parameters;
    Object.assign(body, otherParams);

    // Add a tool
    if (request.tools && request.tools.length > 0) {
      body["tools"] = this.convertTools(request.tools);
    }

    // Add the streaming option (for obtaining complete usage information)
    if (config.stream && config.streamOptions?.includeUsage) {
      body["stream_options"] = this.buildStreamOptions(config);
    }

    // Handle reasoning configuration (for o1, DeepSeek R1, etc.)
    // Users can set parameters.reasoning = { effort: 'high', summary: 'detailed' }
    // The deepMerge will handle this correctly

    return body;
  }

  /**
   * Parse the message
   */
  private parseMessage(message: unknown): LLMMessage {
    const msg = message as Record<string, unknown>;
    return {
      role: msg["role"] as "system" | "user" | "assistant" | "tool",
      content: (msg["content"] as string) || "",
      toolCalls: msg["tool_calls"] ? this.parseToolCalls(msg["tool_calls"]) : undefined,
    };
  }
}
