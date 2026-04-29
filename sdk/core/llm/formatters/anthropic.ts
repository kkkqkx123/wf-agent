/**
 * Anthropic Format Converter
 *
 * Implements the conversion of request and response formats for the Anthropic API
 */

import { BaseFormatter } from "./base.js";
import type { LLMRequest, LLMResult, LLMMessage, LLMToolCall, ToolCallFormat } from "@wf-agent/types";
import type { ToolSchema } from "@wf-agent/types";
import type { FormatterConfig, BuildRequestResult, ParseStreamChunkResult } from "./types.js";
import { convertToolsToAnthropicFormat } from "../utils/index.js";
import { ToolDeclarationFormatter } from "@wf-agent/prompt-templates";
import { getToolCallParserOptions } from "./tool-format-selector.js";
import { ToolCallParser } from "./tool-call-parser.js";
import { HistoryConverter } from "../../messages/history-converter.js";

/**
 * Anthropic format converter
 */
export class AnthropicFormatter extends BaseFormatter {
  private readonly apiVersion: string;

  constructor(apiVersion: string = "2023-06-01") {
    super();
    this.apiVersion = apiVersion;
  }

  getSupportedProvider(): string {
    return "ANTHROPIC";
  }

  /**
   * Build request in native function-calling mode
   */
  protected buildNativeRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult {
    const body = this.buildRequestBody(request, config);

    // Constructing request headers
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      "anthropic-version": (config.profile.metadata?.["apiVersion"] as string) || this.apiVersion,
      "anthropic-dangerous-direct-browser-access": "false",
      ...config.profile.headers,
    };

    // Add authentication headers (supporting both x-api-key and Authorization Bearer methods)
    if (config.profile.apiKey) {
      Object.assign(headers, this.buildAuthHeader(config.profile.apiKey, config, "x-api-key"));
    }

    // Add custom request headers
    Object.assign(headers, this.buildCustomHeaders(config));

    // Apply a custom request body
    const finalBody = this.applyCustomBody(body, config);

    // Construct query parameters
    const queryString = this.buildQueryString(config);

    return {
      httpRequest: {
        url: `/v1/messages${queryString}`,
        method: "POST",
        headers,
        body: finalBody,
        timeout: config.timeout,
      },
      transformedBody: finalBody,
    };
  }

  /**
   * Build request in text-based mode (XML/JSON)
   */
  protected override buildTextModeRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult {
    const format = this.getToolCallFormat(config);

    // 1. Generate tool declarations
    const toolDeclarations = ToolDeclarationFormatter.formatTools(
      request.tools || [],
      {
        format: format === 'json_wrapped' || format === 'json_raw' ? 'json' : 'xml',
        xmlTags: config.toolCallFormat?.xmlTags,
        markers: config.toolCallFormat?.markers,
        includeDescription: config.toolCallFormat?.includeDescription,
      }
    );

    // 2. Convert history to text mode (if needed)
    const convertedMessages = HistoryConverter.convertToTextMode(
      request.messages,
      format,
      {
        xmlTags: config.toolCallFormat?.xmlTags,
        markers: config.toolCallFormat?.markers,
      }
    );

    // 3. For Anthropic, inject tools into system field
    const { systemMessage, filteredMessages } = this.extractSystemMessage(convertedMessages);
    const existingSystem = systemMessage?.content || '';
    const instructions = this.getToolUsageInstructions(format);
    
    const systemContent = `${existingSystem}\n\n${instructions}\n\n${toolDeclarations}`.trim();

    // 3. Build request WITHOUT tools field
    const parameters = this.mergeParameters(config.profile.parameters, request.parameters);
    const body: Record<string, unknown> = {
      model: config.profile.model,
      messages: this.convertMessages(filteredMessages),
      system: systemContent,
      stream: config.stream || false,
    };

    const { stream: _stream, ...otherParams } = parameters;
    Object.assign(body, otherParams);

    // NO tools field!

    // Constructing request headers
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      "anthropic-version": (config.profile.metadata?.["apiVersion"] as string) || this.apiVersion,
      "anthropic-dangerous-direct-browser-access": "false",
      ...config.profile.headers,
    };

    // Add authentication headers
    if (config.profile.apiKey) {
      Object.assign(headers, this.buildAuthHeader(config.profile.apiKey, config, "x-api-key"));
    }

    // Add custom request headers
    Object.assign(headers, this.buildCustomHeaders(config));

    // Apply a custom request body
    const finalBody = this.applyCustomBody(body, config);

    // Construct query parameters
    const queryString = this.buildQueryString(config);

    return {
      httpRequest: {
        url: `/v1/messages${queryString}`,
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
    const content = this.extractContent(dataRecord["content"]);
    const toolCalls = this.extractToolCalls(dataRecord["content"]);

    // Extract thinking content (for Claude extended thinking)
    const thinkingContent = this.extractThinkingContent(dataRecord["content"]);

    const usage = dataRecord["usage"] as Record<string, number> | undefined;

    return {
      id: dataRecord["id"] as string,
      model: dataRecord["model"] as string,
      content,
      message: {
        role: "assistant",
        content,
        toolCalls,
      },
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      usage: usage
        ? {
            promptTokens: usage["input_tokens"] ?? 0,
            completionTokens: usage["output_tokens"] ?? 0,
            totalTokens: (usage["input_tokens"] ?? 0) + (usage["output_tokens"] ?? 0),
          }
        : undefined,
      finishReason: dataRecord["stop_reason"] as string,
      duration: 0,
      metadata: {
        type: dataRecord["type"] as string,
        stopReason: dataRecord["stop_reason"] as string,
      },
      reasoningContent: thinkingContent,
    };
  }

  /**
   * Parse response in text-based mode (XML/JSON)
   */
  protected override parseTextModeResponse(data: unknown, config: FormatterConfig): LLMResult {
    const dataRecord = data as Record<string, unknown>;
    const content = this.extractContent(dataRecord["content"]);
    const format = this.getToolCallFormat(config);

    // Parse tool calls from content using ToolCallParser
    const toolCalls = config.toolCallFormat
      ? ToolCallParser.parseFromText(
          content,
          getToolCallParserOptions(config.toolCallFormat.format, config.toolCallFormat.markers)
        )
      : [];

    // Extract thinking content (for Claude extended thinking)
    const thinkingContent = this.extractThinkingContent(dataRecord["content"]);

    const usage = dataRecord["usage"] as Record<string, number> | undefined;

    return {
      id: dataRecord["id"] as string,
      model: dataRecord["model"] as string,
      content,
      message: {
        role: "assistant",
        content,
        toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      },
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      usage: usage
        ? {
            promptTokens: usage["input_tokens"] ?? 0,
            completionTokens: usage["output_tokens"] ?? 0,
            totalTokens: (usage["input_tokens"] ?? 0) + (usage["output_tokens"] ?? 0),
          }
        : undefined,
      finishReason: dataRecord["stop_reason"] as string,
      duration: 0,
      metadata: {
        type: dataRecord["type"] as string,
        stopReason: dataRecord["stop_reason"] as string,
      },
      reasoningContent: thinkingContent,
    };
  }

  /**
   * Get tool usage instructions based on format
   */
  private getToolUsageInstructions(format: ToolCallFormat): string {
    if (format === 'xml') {
      return `## Tool Usage Instructions

When you need to use a tool, format your response as follows:

<tool_use>
  <tool_name>tool_name_here</tool_name>
  <parameters>
    <param1>value1</param1>
    <param2>value2</param2>
  </parameters>
</tool_use>

You can use multiple tools in one response by including multiple <tool_use> blocks.`;
    } else if (format === 'json_wrapped') {
      return `## Tool Usage Instructions

When you need to use a tool, format your response as follows:

<<<TOOL_CALL>>>
{
  "tool": "tool_name_here",
  "parameters": {
    "param1": "value1",
    "param2": "value2"
  }
}
<<<END_TOOL_CALL>>>

You can use multiple tools in one response by including multiple blocks.`;
    }

    return '';
  }

  parseStreamChunk(data: unknown): ParseStreamChunkResult {
    const dataRecord = data as Record<string, unknown>;
    const delta = dataRecord["delta"] as Record<string, unknown> | undefined;
    const contentBlock = dataRecord["content_block"] as Record<string, unknown> | undefined;
    const usage = dataRecord["usage"] as Record<string, number> | undefined;
    const message = dataRecord["message"] as Record<string, unknown> | undefined;
    const messageUsage = message?.["usage"] as Record<string, number> | undefined;

    switch (dataRecord["type"]) {
      case "content_block_delta":
        // Text delta event
        if (delta && delta["type"] === "text_delta" && delta["text"]) {
          return {
            chunk: {
              delta: delta["text"] as string,
              done: false,
              raw: data,
            },
            valid: true,
          };
        }
        // Thinking delta event (for Claude extended thinking)
        if (delta && delta["type"] === "thinking_delta" && delta["thinking"]) {
          return {
            chunk: {
              delta: "",
              done: false,
              reasoningDelta: delta["thinking"] as string,
              raw: data,
            },
            valid: true,
          };
        }
        break;

      case "content_block_start":
        // Content block start event (tool call)
        if (contentBlock && contentBlock["type"] === "tool_use") {
          const toolCall: LLMToolCall = {
            id: contentBlock["id"] as string,
            type: "function",
            function: {
              name: contentBlock["name"] as string,
              arguments: JSON.stringify(contentBlock["input"] || {}),
            },
          };
          return {
            chunk: {
              delta: "",
              done: false,
              toolCallsDelta: [toolCall],
              raw: { toolCall },
            },
            valid: true,
          };
        }
        // Thinking block start event
        if (contentBlock && contentBlock["type"] === "thinking") {
          return {
            chunk: {
              delta: "",
              done: false,
              raw: data,
            },
            valid: true,
          };
        }
        break;

      case "message_delta":
        // Message delta event (contains usage)
        if (usage) {
          // Handle stop_reason: end_turn, max_tokens, stop_sequence, tool_use
          const stopReason = delta?.["stop_reason"] as string | undefined;
          const isDone =
            stopReason === "end_turn" ||
            stopReason === "max_tokens" ||
            stopReason === "stop_sequence" ||
            stopReason === "tool_use";
          return {
            chunk: {
              delta: "",
              done: isDone,
              usage: {
                promptTokens: usage["input_tokens"] || 0,
                completionTokens: usage["output_tokens"] || 0,
                totalTokens: (usage["input_tokens"] || 0) + (usage["output_tokens"] || 0),
              },
              finishReason: stopReason,
              raw: data,
            },
            valid: true,
          };
        }
        break;

      case "message_start":
        // Message start event
        if (messageUsage) {
          return {
            chunk: {
              delta: "",
              done: false,
              usage: {
                promptTokens: messageUsage["input_tokens"] || 0,
                completionTokens: 0,
                totalTokens: messageUsage["input_tokens"] || 0,
              },
              raw: data,
            },
            valid: true,
          };
        }
        break;

      default:
        break;
    }

    return { chunk: { done: false }, valid: false };
  }

  convertTools(tools: ToolSchema[]): unknown {
    return convertToolsToAnthropicFormat(tools);
  }

  convertMessages(messages: LLMMessage[]): unknown[] {
    return messages
      .filter(msg => msg.role !== "system") // Anthropic does not support system messages in the messages array.
      .map(msg => {
        const converted: { role: string; content: unknown[] } = {
          role: msg.role === "assistant" ? "assistant" : "user",
          content: [],
        };

        // Process the content.
        if (typeof msg.content === "string") {
          converted.content.push({
            type: "text",
            text: msg.content,
          });
        } else if (Array.isArray(msg.content)) {
          converted.content = msg.content;
        }

        // Add the results of the tool calls.
        if (msg.role === "tool" && msg.toolCallId) {
          converted.role = "user";
          converted.content = [
            {
              type: "tool_result",
              tool_use_id: msg.toolCallId,
              content: typeof msg.content === "string" ? msg.content : JSON.stringify(msg.content),
            },
          ];
        }

        // Add tool calls
        if (msg.toolCalls && msg.toolCalls.length > 0) {
          converted.content = msg.toolCalls.map(call => ({
            type: "tool_use",
            id: call.id,
            name: call.function.name,
            input: JSON.parse(call.function.arguments),
          }));
        }

        return converted;
      });
  }

  parseToolCalls(toolCalls: unknown[]): LLMToolCall[] {
    if (!toolCalls) return [];
    return toolCalls.map(call => ({
      id: (call as { id: string }).id,
      type: "function",
      function: {
        name: (call as { name: string }).name,
        arguments:
          typeof (call as { input?: unknown }).input === "string"
            ? (call as { input: string }).input
            : JSON.stringify((call as { input?: unknown }).input || {}),
      },
    }));
  }

  /**
   * Construct a Token count request
   * @param request: LLM request
   * @param config: Format converter configuration
   * @returns: HTTP request configuration
   */
  buildCountTokensRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult {
    const parameters = this.mergeParameters(config.profile.parameters, request.parameters);

    const body: Record<string, unknown> = {
      model: config.profile.model,
    };

    // Processing system messages
    const { systemMessage, filteredMessages } = this.extractSystemMessage(request.messages);
    if (systemMessage) {
      body["system"] =
        typeof systemMessage.content === "string"
          ? systemMessage.content
          : JSON.stringify(systemMessage.content);
    }
    body["messages"] = this.convertMessages(filteredMessages);

    // Add tools (if any).
    if (request.tools && request.tools.length > 0) {
      body["tools"] = this.convertTools(request.tools);
    }

    // Add the thinking configuration (if it exists).
    if (parameters["thinking"]) {
      body["thinking"] = parameters["thinking"];
    }

    // Add tool_choice (if it exists)
    if (parameters["tool_choice"]) {
      body["tool_choice"] = parameters["tool_choice"];
    }

    // Constructing request headers
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      "anthropic-version": (config.profile.metadata?.["apiVersion"] as string) || this.apiVersion,
      "anthropic-dangerous-direct-browser-access": "false",
      ...config.profile.headers,
    };

    // Add authentication headers
    if (config.profile.apiKey) {
      Object.assign(headers, this.buildAuthHeader(config.profile.apiKey, config, "x-api-key"));
    }

    // Add custom request headers
    Object.assign(headers, this.buildCustomHeaders(config));

    return {
      httpRequest: {
        url: "/v1/messages/count_tokens",
        method: "POST",
        headers,
        body,
        timeout: config.timeout,
      },
      transformedBody: body,
    };
  }

  /**
   * Construct the request body
   */
  private buildRequestBody(request: LLMRequest, config: FormatterConfig): Record<string, unknown> {
    const parameters = this.mergeParameters(config.profile.parameters, request.parameters);

    const body: Record<string, unknown> = {
      model: config.profile.model,
      ["max_tokens"]: parameters["max_tokens"] || 4096,
      stream: config.stream || false,
    };

    // Handle system messages
    const { systemMessage, filteredMessages } = this.extractSystemMessage(request.messages);
    if (systemMessage) {
      body["system"] =
        typeof systemMessage.content === "string"
          ? systemMessage.content
          : JSON.stringify(systemMessage.content);
    }
    body["messages"] = this.convertMessages(filteredMessages);

    // Merge other parameters (excluding stream and max_tokens, which have been handled separately).
    const { stream: _stream, max_tokens: _max_tokens, ...otherParams } = parameters;
    Object.assign(body, otherParams);

    // Add tools
    if (request.tools && request.tools.length > 0) {
      body["tools"] = this.convertTools(request.tools);
    }

    // Handle thinking configuration (for Claude extended thinking)
    // Users can set parameters.thinking = { type: 'enabled', budget_tokens: 10000 }
    // or parameters.thinking = { type: 'adaptive', effort: 'high' }
    // The deepMerge will handle this correctly

    return body;
  }

  /**
   * Extract the text content.
   */
  private extractContent(content: unknown): string {
    if (!content || !Array.isArray(content)) {
      return "";
    }

    return content
      .filter(item => (item as { type?: string }).type === "text")
      .map(item => (item as { text?: string }).text ?? "")
      .join("");
  }

  /**
   * Extract tool calls
   */
  private extractToolCalls(content: unknown): LLMToolCall[] {
    if (!content || !Array.isArray(content)) {
      return [];
    }

    return content
      .filter(item => (item as { type?: string }).type === "tool_use")
      .map(item => ({
        id: (item as { id: string }).id,
        type: "function",
        function: {
          name: (item as { name: string }).name,
          arguments: JSON.stringify((item as { input?: unknown }).input || {}),
        },
      }));
  }

  /**
   * Extract thinking content blocks from Anthropic responses
   *
   *
   */
  private extractThinkingContent(content: unknown): string | undefined {
    if (!content || !Array.isArray(content)) {
      return undefined;
    }

    const thinkingBlocks = content.filter(item => (item as { type?: string }).type === "thinking");
    if (thinkingBlocks.length === 0) {
      return undefined;
    }

    return thinkingBlocks.map(block => (block as { thinking?: string }).thinking || "").join("\n");
  }
}
