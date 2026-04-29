/**
 * Gemini OpenAI Compatible Format Converter
 *
 * Implements the conversion of request and response formats compatible with the Gemini OpenAI API
 * Uses the Gemini OpenAI-compatible endpoints
 * Supports special parameters such as thinking_budget and cached_content
 */

import { BaseFormatter } from "./base.js";
import type { LLMRequest, LLMResult, LLMMessage, LLMToolCall } from "@wf-agent/types";
import type { ToolSchema } from "@wf-agent/types";
import type { FormatterConfig, BuildRequestResult, ParseStreamChunkResult } from "./types.js";
import { generateId } from "@wf-agent/common-utils";
import { convertToolsToOpenAIFormat } from "../utils/index.js";

/**
 * Gemini OpenAI Compatible Format Converter
 */
export class GeminiOpenAIFormatter extends BaseFormatter {
  override getSupportedProvider(): string {
    return "GEMINI_OPENAI";
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
        url: `/chat/completions${queryString}`,
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
    const candidates = dataRecord["candidates"] as Array<Record<string, unknown>> | undefined;
    const candidate = candidates?.[0];
    if (!candidate) {
      throw new Error("No candidate in response");
    }

    const candidateContent = candidate["content"] as Record<string, unknown> | undefined;
    const parts = (candidateContent?.["parts"] as unknown[]) || [];
    const content = this.extractContent(parts);
    const toolCalls = this.extractToolCalls(parts);

    const usageMetadata = dataRecord["usageMetadata"] as Record<string, number> | undefined;

    return {
      id: (dataRecord["id"] as string) || "unknown",
      model: config.profile.model,
      content,
      message: {
        role: "assistant",
        content,
        toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      },
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      usage: usageMetadata
        ? {
            promptTokens: usageMetadata["promptTokenCount"] || 0,
            completionTokens: usageMetadata["candidatesTokenCount"] || 0,
            totalTokens: usageMetadata["totalTokenCount"] || 0,
          }
        : undefined,
      finishReason: (candidate["finishReason"] as string) || "stop",
      duration: 0,
      metadata: {
        finishReason: candidate["finishReason"],
        safetyRatings: candidate["safetyRatings"],
      },
    };
  }

  parseStreamChunk(data: unknown): ParseStreamChunkResult {
    const dataRecord = data as Record<string, unknown>;
    const candidates = dataRecord["candidates"] as Array<Record<string, unknown>> | undefined;
    const candidate = candidates?.[0];
    if (!candidate) {
      return { chunk: { done: false }, valid: false };
    }

    const candidateContent = candidate["content"] as Record<string, unknown> | undefined;
    const parts = (candidateContent?.["parts"] as unknown[]) || [];
    const content = this.extractContent(parts);
    // Extract tool calls from the content parts
    const toolCallsDelta = this.extractToolCalls(parts);

    const usageMetadata = dataRecord["usageMetadata"] as Record<string, number> | undefined;

    // Handle finishReason: STOP, MAX_TOKENS, SAFETY, RECITATION, OTHER
    const finishReason = candidate["finishReason"] as string;
    const isDone =
      finishReason === "STOP" ||
      finishReason === "MAX_TOKENS" ||
      finishReason === "SAFETY" ||
      finishReason === "RECITATION" ||
      finishReason === "OTHER";

    return {
      chunk: {
        delta: content,
        done: isDone,
        usage: usageMetadata
          ? {
              promptTokens: usageMetadata["promptTokenCount"] || 0,
              completionTokens: usageMetadata["candidatesTokenCount"] || 0,
              totalTokens: usageMetadata["totalTokenCount"] || 0,
            }
          : undefined,
        finishReason,
        raw: data,
        toolCallsDelta: toolCallsDelta.length > 0 ? toolCallsDelta : undefined,
      },
      valid: true,
    };
  }

  convertTools(tools: ToolSchema[]): unknown {
    return convertToolsToOpenAIFormat(tools);
  }

  /**
   * The Gemini OpenAI-compatible API uses the OpenAI format but supports parameters specific to Gemini.
   *
   */
  convertMessages(messages: LLMMessage[]): unknown[] {
    return messages
      .filter(msg => msg.role !== "system") // The Gemini OpenAI-compatible API uses `systemInstruction` to process system messages.
      .map(msg => {
        const converted: {
          role: string;
          content: unknown[];
          tool_calls?: unknown[];
          name?: string;
        } = {
          role: msg.role === "assistant" ? "model" : "user",
          content: [],
        };

        // Process the content.
        if (typeof msg.content === "string") {
          converted.content.push({
            text: msg.content,
          });
        } else if (Array.isArray(msg.content)) {
          converted.content = msg.content;
        }

        // Processing the results of tool calls.
        if (msg.role === "tool" && msg.toolCallId) {
          converted.role = "user";
          converted.content = [
            {
              functionResponse: {
                name: msg.toolCallId,
                response: {
                  result:
                    typeof msg.content === "string"
                      ? msg.content
                      : JSON.parse(JSON.stringify(msg.content)),
                },
              },
            },
          ];
        }

        // Handle tool calls
        if (msg.toolCalls && msg.toolCalls.length > 0) {
          converted.content = msg.toolCalls.map(call => ({
            functionCall: {
              name: call.function.name,
              args: JSON.parse(call.function.arguments),
            },
          }));
        }

        return converted;
      });
  }

  parseToolCalls(toolCalls: unknown[]): LLMToolCall[] {
    if (!toolCalls) return [];
    return toolCalls.map(call => ({
      id: (call as { id?: string }).id || generateId(),
      type: "function",
      function: {
        name:
          (call as { name?: string }).name ||
          (call as { functionCall?: { name?: string } }).functionCall?.name ||
          "",
        arguments:
          typeof (call as { args?: unknown }).args === "string"
            ? (call as { args: string }).args
            : JSON.stringify(
                (call as { args?: unknown }).args ||
                  (call as { functionCall?: { args?: unknown } }).functionCall?.args ||
                  {},
              ),
      },
    }));
  }

  /**
   * Constructing the request body
   * The Gemini OpenAI-compatible API uses the OpenAI format, but it also supports parameters specific to Gemini.
   */
  private buildRequestBody(request: LLMRequest, config: FormatterConfig): Record<string, unknown> {
    const parameters = this.mergeParameters(config.profile.parameters, request.parameters);

    const body: Record<string, unknown> = {
      model: config.profile.model,
      messages: this.convertMessages(request.messages),
      stream: config.stream || false,
    };

    // Merge parameters
    const { stream: _stream, ...otherParams } = parameters;
    Object.assign(body, otherParams);

    // Add tools
    if (request.tools && request.tools.length > 0) {
      body["tools"] = this.convertTools(request.tools);
    }

    return body;
  }

  /**
   * Extract the text content.
   */
  private extractContent(parts: unknown): string {
    if (!parts || !Array.isArray(parts)) {
      return "";
    }

    return parts
      .filter(part => (part as { text?: string }).text)
      .map(part => (part as { text: string }).text)
      .join("");
  }

  /**
   * Extract tool calls
   */
  private extractToolCalls(parts: unknown): LLMToolCall[] {
    if (!parts || !Array.isArray(parts)) {
      return [];
    }

    return parts
      .filter(part => (part as { functionCall?: unknown }).functionCall)
      .map(part => ({
        id: generateId(),
        type: "function",
        function: {
          name: (part as { functionCall: { name: string } }).functionCall.name,
          arguments: JSON.stringify(
            (part as { functionCall: { args?: unknown } }).functionCall.args || {},
          ),
        },
      }));
  }
}
