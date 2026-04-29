/**
 * OpenAI Response API Formatter
 *
 * Converts the request and response formats of the OpenAI Response API
 * Uses the /responses endpoint
 * Supports special parameters such as reasoning_effort and previous_response_id
 */

import { BaseFormatter } from "./base.js";
import type { LLMRequest, LLMResult, LLMMessage, LLMToolCall } from "@wf-agent/types";
import type { ToolSchema } from "@wf-agent/types";
import type { FormatterConfig, BuildRequestResult, ParseStreamChunkResult } from "./types.js";
import { convertToolsToOpenAIFormat } from "../utils/index.js";

/**
 * OpenAI Response API Format Converter
 */
export class OpenAIResponseFormatter extends BaseFormatter {
  override getSupportedProvider(): string {
    return "OPENAI_RESPONSE";
  }

  /**
   * Build request in native function-calling mode
   */
  protected buildNativeRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult {
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

    // Apply a custom request body
    const finalBody = this.applyCustomBody(body, config);

    // Construct query parameters
    const queryString = this.buildQueryString(config);

    return {
      httpRequest: {
        url: `/responses${queryString}`,
        method: "POST",
        headers,
        body: finalBody,
        timeout: config.timeout,
      },
      transformedBody: finalBody,
    };
  }

  /**
   * Parse response in native function-calling mode
   */
  protected parseNativeResponse(data: unknown, config: FormatterConfig): LLMResult {
    const dataRecord = data as Record<string, unknown>;
    const output = (dataRecord["output"] as Array<Record<string, unknown>>) || [];
    const lastOutput = output[output.length - 1] || {};
    const lastOutputContent = (lastOutput["content"] as Array<Record<string, unknown>>) || [];
    const firstContent = lastOutputContent[0] || {};
    const usage = dataRecord["usage"] as Record<string, number> | undefined;

    return {
      id: dataRecord["id"] as string,
      model: dataRecord["model"] as string,
      content: (firstContent["text"] as string) || "",
      message: {
        role: "assistant",
        content: (firstContent["text"] as string) || "",
        toolCalls: lastOutput["tool_calls"]
          ? this.parseToolCalls(lastOutput["tool_calls"] as unknown[])
          : undefined,
      },
      toolCalls: lastOutput["tool_calls"]
        ? this.parseToolCalls(lastOutput["tool_calls"] as unknown[])
        : undefined,
      usage: usage
        ? {
            promptTokens: usage["input_tokens"] || 0,
            completionTokens: usage["output_tokens"] || 0,
            totalTokens: (usage["input_tokens"] || 0) + (usage["output_tokens"] || 0),
          }
        : undefined,
      finishReason: (dataRecord["status"] as string) || "completed",
      duration: 0,
      metadata: {
        status: dataRecord["status"],
        created: dataRecord["created_at"],
        previousResponseId: dataRecord["previous_response_id"],
      },
    };
  }

  parseStreamChunk(data: unknown): ParseStreamChunkResult {
    const dataRecord = data as Record<string, unknown>;
    const output = (dataRecord["output"] as Array<Record<string, unknown>>) || [];
    const lastOutput = output[output.length - 1] || {};
    const lastOutputContent = (lastOutput["content"] as Array<Record<string, unknown>>) || [];
    const firstContent = lastOutputContent[0] || {};
    const usage = dataRecord["usage"] as Record<string, number> | undefined;

    // Extract tool calls from streaming chunks
    const toolCallsDelta = lastOutput["tool_calls"]
      ? this.parseToolCalls(lastOutput["tool_calls"] as unknown[])
      : undefined;

    // Handle status: completed, incomplete, in_progress
    // Response API uses status field instead of finish_reason
    const status = dataRecord["status"] as string;
    const isDone = status === "completed" || status === "incomplete";

    return {
      chunk: {
        delta: (firstContent["text"] as string) || "",
        done: isDone,
        usage: usage
          ? {
              promptTokens: usage["input_tokens"] || 0,
              completionTokens: usage["output_tokens"] || 0,
              totalTokens: (usage["input_tokens"] || 0) + (usage["output_tokens"] || 0),
            }
          : undefined,
        finishReason: status,
        modelVersion: dataRecord["model"] as string,
        raw: data,
        toolCallsDelta,
      },
      valid: true,
    };
  }

  convertTools(tools: ToolSchema[]): unknown {
    return convertToolsToOpenAIFormat(tools);
  }

  /**
   * The response API uses the `input` field instead of `messages`.
   *
   */
  convertMessages(messages: LLMMessage[]): unknown[] {
    return messages.map(msg => {
      const converted: {
        role: string;
        content: unknown;
        tool_calls?: unknown[];
        tool_call_id?: string;
      } = {
        role: msg.role,
        content: msg.content,
      };

      // Add a tool call
      if (msg.toolCalls && msg.toolCalls.length > 0) {
        converted.tool_calls = msg.toolCalls.map(call => ({
          id: call.id,
          type: call.type as "function",
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

  parseToolCalls(toolCalls: unknown[]): LLMToolCall[] {
    return toolCalls.map(call => ({
      id: (call as { id: string }).id,
      type: "function" as const,
      function: {
        name:
          (call as { function?: { name?: string }; name?: string }).function?.name ||
          (call as { name?: string }).name ||
          "",
        arguments:
          typeof (call as { function?: { arguments?: string } }).function?.arguments === "string"
            ? (call as { function: { arguments: string } }).function.arguments
            : typeof (call as { arguments?: string }).arguments === "string"
              ? (call as { arguments: string }).arguments
              : JSON.stringify(
                  (call as { function?: { arguments?: unknown }; arguments?: unknown }).function
                    ?.arguments ||
                    (call as { arguments?: unknown }).arguments ||
                    {},
                ),
      },
    }));
  }

  /**
   * Constructing the request body: The Response API uses different request formats.
   *
   */
  private buildRequestBody(request: LLMRequest, config: FormatterConfig): Record<string, unknown> {
    const parameters = this.mergeParameters(config.profile.parameters, request.parameters);

    const body: Record<string, unknown> = {
      model: config.profile.model,
      input: this.convertMessages(request.messages),
      stream: config.stream || false,
    };

    // Merge parameters (both special parameters and general parameters are merged directly)
    const { stream: _stream, ...otherParams } = parameters;
    Object.assign(body, otherParams);

    // Add tools
    if (request.tools && request.tools.length > 0) {
      body["tools"] = this.convertTools(request.tools);
    }

    return body;
  }
}
