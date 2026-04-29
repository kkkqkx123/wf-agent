/**
 * OpenAI Chat Format Converter
 *
 * Implement the conversion of request and response formats for the OpenAI Chat API
 * Use the /chat/completions endpoint
 */

import { BaseFormatter } from "./base.js";
import type { LLMRequest, LLMResult, LLMMessage, LLMToolCall, ToolCallFormat } from "@wf-agent/types";
import type { ToolSchema } from "@wf-agent/types";
import type { FormatterConfig, BuildRequestResult, ParseStreamChunkResult } from "./types.js";
import { convertToolsToOpenAIFormat } from "../utils/index.js";
import { ToolDeclarationFormatter } from "@wf-agent/prompt-templates";
import { getToolCallParserOptions } from "./tool-format-selector.js";
import { ToolCallParser } from "./tool-call-parser.js";
import { HistoryConverter } from "../../messages/history-converter.js";

/**
 * OpenAI Chat Format Converter
 */
export class OpenAIChatFormatter extends BaseFormatter {
  getSupportedProvider(): string {
    return "OPENAI_CHAT";
  }

  /**
   * Parse response in native function-calling mode
   */
  protected parseNativeResponse(data: unknown, config: FormatterConfig): LLMResult {
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
   * Parse response in text-based mode (XML/JSON)
   */
  protected override parseTextModeResponse(data: unknown, config: FormatterConfig): LLMResult {
    const dataRecord = data as Record<string, unknown>;
    const choices = dataRecord["choices"] as Array<Record<string, unknown>> | undefined;
    const choice = choices?.[0];
    const message = choice?.["message"] as Record<string, unknown> | undefined;

    const content = (message?.["content"] as string) || "";
    const format = this.getToolCallFormat(config);

    // Parse tool calls from content using ToolCallParser
    const toolCalls = ToolCallParser.parseFromText(
      content,
      getToolCallParserOptions(format, config.toolCallFormat?.markers)
    );

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
      content,
      message: {
        role: "assistant",
        content,
        toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      },
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      usage: usage
        ? {
            promptTokens: usage["prompt_tokens"] as number,
            completionTokens: usage["completion_tokens"] as number,
            totalTokens: usage["total_tokens"] as number,
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

  /**
   * Inject tool declarations into system message
   */
  private injectToolDeclarations(
    messages: LLMMessage[],
    toolDeclarations: string,
    format: ToolCallFormat
  ): LLMMessage[] {
    const instructions = this.getToolUsageInstructions(format);
    const fullInjection = `${instructions}\n\n${toolDeclarations}`;

    const systemMsgIndex = messages.findIndex(m => m.role === 'system');

    if (systemMsgIndex >= 0) {
      const updated = [...messages];
      const msg = updated[systemMsgIndex];
      if (!msg) {
        return messages;
      }
      const existingContent = msg.content;
      updated[systemMsgIndex] = {
        role: msg.role,
        content: typeof existingContent === 'string' 
          ? `${existingContent}\n\n${fullInjection}`
          : fullInjection,
      };
      return updated;
    } else {
      return [
        {
          role: 'system',
          content: fullInjection,
        },
        ...messages,
      ];
    }
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

    if (config.profile.apiKey) {
      Object.assign(headers, this.buildAuthHeader(config.profile.apiKey, config, "Authorization"));
    }

    Object.assign(headers, this.buildCustomHeaders(config));
    const finalBody = this.applyCustomBody(body, config);
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

    // 3. Inject tool declarations into system message
    const messagesWithTools = this.injectToolDeclarations(
      convertedMessages,
      toolDeclarations,
      format
    );

    // 4. Build request WITHOUT tools field
    const parameters = this.mergeParameters(config.profile.parameters, request.parameters);
    const body: Record<string, unknown> = {
      model: config.profile.model,
      messages: this.convertMessages(messagesWithTools),
      stream: config.stream || false,
    };

    const { stream: _stream, ...otherParams } = parameters;
    Object.assign(body, otherParams);

    // NO tools field!

    if (config.stream && config.streamOptions?.includeUsage) {
      body["stream_options"] = this.buildStreamOptions(config);
    }

    // Constructing request headers
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      ...config.profile.headers,
    };

    if (config.profile.apiKey) {
      Object.assign(headers, this.buildAuthHeader(config.profile.apiKey, config, "Authorization"));
    }

    Object.assign(headers, this.buildCustomHeaders(config));
    const finalBody = this.applyCustomBody(body, config);
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
   * Construct the request body (used by native mode)
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
