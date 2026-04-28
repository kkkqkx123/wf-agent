/**
 * Gemini Native Format Converter
 *
 * Implements the conversion of request and response formats for the Gemini Native API
 * Uses Gemini's native endpoints
 */

import { BaseFormatter } from "./base.js";
import type { LLMRequest, LLMResult, LLMMessage, LLMToolCall } from "@wf-agent/types";
import type { ToolSchema } from "@wf-agent/types";
import type { FormatterConfig, BuildRequestResult, ParseStreamChunkResult } from "./types.js";
import { generateId } from "@wf-agent/common-utils";
import { convertToolsToGeminiFormat } from "../utils/index.js";
import { extractAndFilterSystemMessages } from "../message-helper.js";

/**
 * Gemini Native Format Converter
 */
export class GeminiNativeFormatter extends BaseFormatter {
  getSupportedProvider(): string {
    return "GEMINI_NATIVE";
  }

  buildRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult {
    const body = this.buildRequestBody(request, config);

    // Construct endpoint paths
    const method = config.stream ? "streamGenerateContent" : "generateContent";
    const endpoint = `/models/${config.profile.model}:${method}`;

    // The Gemini Native API uses query parameters to pass the API Key.
    const queryParams: Record<string, string | number | boolean> = {
      key: config.profile.apiKey || "",
    };

    // Streaming requests require the addition of the `alt=sse` parameter to obtain a response in SSE format.
    if (config.stream) {
      queryParams["alt"] = "sse";
    }

    // Merge user-defined query parameters
    if (config.queryParams) {
      Object.assign(queryParams, config.queryParams);
    }

    // Constructing request headers
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      ...config.profile.headers,
    };

    // Add authentication headers (supporting both x-goog-api-key and Authorization Bearer methods).
    if (config.profile.apiKey) {
      Object.assign(headers, this.buildAuthHeader(config.profile.apiKey, config, "x-goog-api-key"));
    }

    // Add custom request headers
    Object.assign(headers, this.buildCustomHeaders(config));

    // Apply a custom request body
    const finalBody = this.applyCustomBody(body, config);

    return {
      httpRequest: {
        url: endpoint,
        method: "POST",
        headers,
        query: queryParams,
        body: finalBody,
        timeout: config.timeout,
      },
      transformedBody: finalBody,
    };
  }

  parseResponse(data: unknown, config: FormatterConfig): LLMResult {
    const dataRecord = data as Record<string, unknown>;
    const candidates = dataRecord["candidates"] as Array<Record<string, unknown>> | undefined;
    const candidate = candidates?.[0];
    if (!candidate) {
      throw new Error("No candidate in response");
    }

    const candidateContent = candidate["content"] as Record<string, unknown> | undefined;
    const parts = (candidateContent?.["parts"] as unknown[]) || [];
    const content = this.extractContent(parts);
    // Extract tool calls for potential future use
    const toolCalls = this.extractToolCalls(parts);

    // Extract thinking content (for Gemini thinking models)
    const thinkingContent = this.extractThinkingContent(parts);

    // Extract thoughtsTokenCount for thinking models
    const usageMetadata = dataRecord["usageMetadata"] as Record<string, number> | undefined;
    const thoughtsTokenCount = usageMetadata?.["thoughtsTokenCount"] || 0;

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
            // Add the `reasoningTokens` field to map Gemini's `thoughtsTokenCount`
            reasoningTokens: thoughtsTokenCount > 0 ? thoughtsTokenCount : undefined,
          }
        : undefined,
      finishReason: (candidate["finishReason"] as string) || "stop",
      duration: 0,
      metadata: {
        finishReason: candidate["finishReason"],
        safetyRatings: candidate["safetyRatings"],
      },
      reasoningContent: thinkingContent,
      reasoningTokens: thoughtsTokenCount > 0 ? thoughtsTokenCount : undefined,
    };
  }

  /**
   * Parsing stream response lines (rewritten)
   *
   * The Gemini Native API returns JSON directly, without the `data:` prefix.
   */
  override parseStreamLine(line: string, config: FormatterConfig): ParseStreamChunkResult {
    // Skip blank lines
    if (!line) {
      return { chunk: { done: false }, valid: false };
    }

    // The Gemini Native API returns JSON directly, without the "data:" prefix.
    try {
      const data = JSON.parse(line);
      return this.parseStreamChunk(data);
    } catch {
      // Skip invalid JSON
      return { chunk: { done: false }, valid: false };
    }
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
    // Extract tool calls from streaming chunks
    const toolCallsDelta = this.extractToolCalls(parts);

    // Extract thinking content delta (for Gemini thinking models)
    const thinkingDelta = this.extractThinkingDelta(parts);

    // Extract thoughtsTokenCount for thinking models
    const usageMetadata = dataRecord["usageMetadata"] as Record<string, number> | undefined;
    const thoughtsTokenCount = usageMetadata?.["thoughtsTokenCount"] || 0;

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
              // Add the `reasoningTokens` field to map Gemini's `thoughtsTokenCount`
              reasoningTokens: thoughtsTokenCount > 0 ? thoughtsTokenCount : undefined,
            }
          : undefined,
        finishReason,
        raw: data,
        reasoningDelta: thinkingDelta,
        toolCallsDelta: toolCallsDelta.length > 0 ? toolCallsDelta : undefined,
      },
      valid: true,
    };
  }

  convertTools(tools: ToolSchema[]): unknown {
    return convertToolsToGeminiFormat(tools);
  }

  /**
   * Convert message format
   */
  convertMessages(messages: LLMMessage[]): unknown[] {
    return messages
      .filter(msg => msg.role !== "system") // Gemini uses systemInstruction to handle system messages.
      .map(msg => {
        const converted: { role: string; parts: unknown[] } = {
          role: msg.role === "assistant" ? "model" : "user",
          parts: [],
        };

        // Process the content.
        if (typeof msg.content === "string") {
          converted.parts.push({
            text: msg.content,
          });
        } else if (Array.isArray(msg.content)) {
          converted.parts = msg.content;
        }

        // Processing the results of tool calls
        if (msg.role === "tool" && msg.toolCallId) {
          converted.role = "user";
          converted.parts = [
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
          converted.parts = msg.toolCalls.map(call => ({
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
   * Construct a request body
   */
  private buildRequestBody(request: LLMRequest, config: FormatterConfig): Record<string, unknown> {
    const parameters = this.mergeParameters(config.profile.parameters, request.parameters);

    const body: Record<string, unknown> = {
      contents: [],
      generationConfig: {
        temperature: parameters["temperature"] || 0.7,
        maxOutputTokens: parameters["max_tokens"] || 4096,
        topP: parameters["top_p"] || 1.0,
        topK: parameters["top_k"] || 40,
      },
    };

    // Processing system instructions
    const { systemMessage, filteredMessages } = extractAndFilterSystemMessages(request.messages);
    if (systemMessage) {
      body["systemInstruction"] = {
        parts: [
          {
            text:
              typeof systemMessage.content === "string"
                ? systemMessage.content
                : JSON.stringify(systemMessage.content),
          },
        ],
      };
    }
    body["contents"] = this.convertMessages(filteredMessages);

    // Add tools
    if (request.tools && request.tools.length > 0) {
      body["tools"] = this.convertTools(request.tools);
    }

    // Handle thinking configuration (for Gemini thinking models)
    // Users can set parameters.thinkingConfig = { includeThoughts: true, thinkingLevel: 3 }
    // The deepMerge will handle this correctly

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
      .map(part => (part as { text?: string }).text ?? "")
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
          name: (part as { functionCall?: { name?: string } }).functionCall?.name ?? "",
          arguments: JSON.stringify(
            (part as { functionCall?: { args?: unknown } }).functionCall?.args || {},
          ),
        },
      }));
  }

  /**
   * Extract thought content
   *
   * Extract content of the thought type from the Gemini response
   */
  private extractThinkingContent(parts: unknown): string | undefined {
    if (!parts || !Array.isArray(parts)) {
      return undefined;
    }

    const thoughtParts = parts.filter(part => (part as { thought?: unknown }).thought);
    if (thoughtParts.length === 0) {
      return undefined;
    }

    return thoughtParts.map(part => (part as { text?: string }).text ?? "").join("\n");
  }

  /**
   * Extract incremental content of the thought type from Gemini's streaming response
   *
   *
   */
  private extractThinkingDelta(parts: unknown): string | undefined {
    if (!parts || !Array.isArray(parts)) {
      return undefined;
    }

    const thoughtParts = parts.filter(part => (part as { thought?: unknown }).thought);
    if (thoughtParts.length === 0) {
      return undefined;
    }

    return thoughtParts.map(part => (part as { text?: string }).text ?? "").join("");
  }
}
