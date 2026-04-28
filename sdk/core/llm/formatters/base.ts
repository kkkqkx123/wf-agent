/**
 * Formatter Abstract Base Class
 *
 * Defines the general interface and implementation for format converters
 * Format converters from all providers inherit from this class
 */

import type { LLMRequest, LLMResult, LLMMessage, LLMToolCall } from "@wf-agent/types";
import { sdkLogger as logger } from "../../../utils/logger.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import type { ToolSchema } from "@wf-agent/types";
import type { FormatterConfig, BuildRequestResult, ParseStreamChunkResult } from "./types.js";
import { ToolCallParser } from "./tool-call-parser.js";
import type { ToolCallParseOptions } from "./types.js";

/**
 * Format Converter Abstract Base Class
 *
 * Responsibilities:
 * 1. Convert the unified LLMRequest into the HTTP request format of a specific provider.
 * 2. Convert the HTTP response from a specific provider into the unified LLMResult format.
 * 3. Handle the parsing of streaming responses.
 *
 * Design Principles:
 * - Single Responsibility: Responsible only for format conversion, not for network requests.
 * - Extensibility: Subclasses only need to implement the conversion logic for specific providers.
 * - Testability: Pure functions, making them easy to unit-test.
 */
export abstract class BaseFormatter {
  /**
   * Get the supported provider types
   */
  abstract getSupportedProvider(): string;

  /**
   * Construct an HTTP request
   *
   * @param request: A unified LLM request
   * @param config: Configuration for the format converter
   * @returns: HTTP request options
   */
  abstract buildRequest(request: LLMRequest, config: FormatterConfig): BuildRequestResult;

  /**
   * Parse non-streaming responses
   *
   * @param data: Raw response data
   * @param config: Format converter configuration
   * @returns: LLM results
   */
  abstract parseResponse(data: unknown, config: FormatterConfig): LLMResult;

  /**
   * Parse Streaming Response Blocks
   *
   * @param data Raw streaming data
   * @param config Format converter configuration
   * @returns Result of the streaming block parsing
   */
  abstract parseStreamChunk(data: unknown, config: FormatterConfig): ParseStreamChunkResult;

  /**
   * Parsing Streaming Response Lines
   *
   * Default implementation: Handles SSE format (data: {...})
   * Subclasses can override this to support other formats.
   *
   * @param line: A line of text from the streaming response
   * @param config: Format converter configuration
   * @returns: The result of parsing the streaming block
   */
  parseStreamLine(line: string, config: FormatterConfig): ParseStreamChunkResult {
    // Skip blank lines
    if (!line) {
      return { chunk: { done: false }, valid: false };
    }

    // Skip the closing tag (OpenAI format)
    if (line === "data: [DONE]") {
      return { chunk: { done: true }, valid: false };
    }

    // Parse data: Prefix
    if (!line.startsWith("data: ")) {
      return { chunk: { done: false }, valid: false };
    }

    const dataStr = line.slice(6);
    try {
      const data = JSON.parse(dataStr);
      return this.parseStreamChunk(data, config);
    } catch {
      // Skip invalid JSON
      return { chunk: { done: false }, valid: false };
    }
  }

  /**
   * Verify Configuration
   *
   * @param config Configuration of the format converter
   * @returns Whether it is valid
   */
  validateConfig(config: FormatterConfig): boolean {
    if (!config.profile) {
      return false;
    }
    if (!config.profile.model) {
      return false;
    }
    return true;
  }

  /**
   * The conversion tool is defined in the format specific to a particular provider.
   *
   * @param tools An array of tool definitions
   * @returns The tool formats for the specific providers
   */
  abstract convertTools(tools: ToolSchema[]): unknown;

  /**
   * Convert messages to a specific provider format
   *
   * @param messages Array of messages
   * @returns Messages in the format of the specific provider
   */
  abstract convertMessages(messages: LLMMessage[]): unknown;

  /**
   * Tool invocation parsing
   *
   * @param data: Tool invocation data from a specific provider
   * @returns: Unified tool invocation format
   */
  abstract parseToolCalls(data: unknown): LLMToolCall[];

  /**
   * Extract system messages
   *
   * @param messages Array of messages
   * @returns System messages and filtered messages
   */
  protected extractSystemMessage(messages: LLMMessage[]): {
    systemMessage: LLMMessage | null;
    filteredMessages: LLMMessage[];
  } {
    const systemMessages = messages.filter(msg => msg.role === "system");
    const filteredMessages = messages.filter(msg => msg.role !== "system");

    return {
      systemMessage: systemMessages.length > 0 ? systemMessages[systemMessages.length - 1]! : null,
      filteredMessages,
    };
  }

  /**
   * Find the index of the last group of user messages
   *
   * Used for determining the insertion position of dynamic context messages
   *
   * @param messages Array of messages
   * @returns The starting index of the last group of user messages
   */
  protected findLastUserMessageGroupIndex(messages: LLMMessage[]): number {
    // Find the last group of consecutive user messages from the end to the beginning.
    for (let i = messages.length - 1; i >= 0; i--) {
      if (messages[i]!.role === "user") {
        // Continue to search for the starting position of this group of user messages.
        while (i > 0 && messages[i - 1]!.role === "user") {
          i--;
        }
        return i;
      }
    }
    return -1;
  }

  /**
   * Clean up internal fields
   *
   * Remove internal fields from the messages that should not be sent to the API
   *
   * @param messages Array of messages
   * @returns Array of cleaned messages
   */
  protected cleanInternalFields(messages: LLMMessage[]): LLMMessage[] {
    return messages.map(msg => {
      const cleaned: LLMMessage = {
        role: msg.role,
        content: msg.content,
      };

      // Retain fields related to tool calls.
      if (msg.toolCalls) {
        cleaned.toolCalls = msg.toolCalls;
      }
      if (msg.toolCallId) {
        cleaned.toolCallId = msg.toolCallId;
      }

      return cleaned;
    });
  }

  /**
   * Merge request parameters
   *
   * @param profileParams Parameters from the Profile
   * @param requestParams Parameters from the request
   * @returns The merged parameters
   */
  protected mergeParameters(
    profileParams: Record<string, unknown> = {},
    requestParams: Record<string, unknown> = {},
  ): Record<string, unknown> {
    return this.deepMerge({ ...profileParams }, requestParams) as Record<string, unknown>;
  }

  /**
   * Deep merge of two objects
   *
   * Used for merging request parameters, supporting deep merging of nested objects.
   *
   * @param target: The target object
   * @param source: The source object
   * @returns: The merged object
   */
  protected deepMerge(target: unknown, source: unknown): unknown {
    if (source === null || source === undefined) {
      return target;
    }

    // If target is an array, merge source into it
    if (Array.isArray(target)) {
      const sourceItems = Array.isArray(source) ? source : [source];
      return [...target, ...sourceItems];
    }

    // If source is an array (but target is not), use override strategy
    if (Array.isArray(source)) {
      return source;
    }

    // If source is not an object, override directly
    if (typeof source !== "object") {
      return source;
    }

    // If target is not an object, initialize as empty object
    if (typeof target !== "object" || target === null) {
      target = {} as Record<string, unknown>;
    }

    const result = { ...(target as Record<string, unknown>) };
    const sourceRecord = source as Record<string, unknown>;

    for (const key of Object.keys(sourceRecord)) {
      // Recursively merge all child nodes
      result[key] = this.deepMerge(result[key], sourceRecord[key]);
    }

    return result;
  }

  /**
   * Construct an authentication header
   *
   * Select the authentication method based on the configuration
   *
   * @param apiKey API Key
   * @param config Configuration for the format converter
   * @param nativeHeaderName Name of the native authentication header (e.g., 'x-api-key', 'x-goog-api-key')
   * @returns Authentication header key-value pairs
   */
  protected buildAuthHeader(
    apiKey: string | undefined,
    config: FormatterConfig,
    nativeHeaderName: string,
  ): Record<string, string> {
    if (!apiKey) {
      return {};
    }

    const authType = config.authType || "native";

    if (authType === "bearer") {
      return { Authorization: `Bearer ${apiKey}` };
    } else {
      return { [nativeHeaderName]: apiKey };
    }
  }

  /**
   * Construct custom request headers
   *
   * Merge custom request header configurations
   *
   * @param config Format converter configuration
   * @returns Custom request headers
   */
  protected buildCustomHeaders(config: FormatterConfig): Record<string, string> {
    const headers: Record<string, string> = {};

    // Handling a simplified version of custom request headers
    if (config.customHeaders) {
      Object.assign(headers, config.customHeaders);
    }

    // Handle the complete version of custom request headers.
    if (config.customHeadersList && config.customHeadersList.length > 0) {
      for (const header of config.customHeadersList) {
        // Add only the enabled request headers with named keys.
        if (header.enabled !== false && header.key && header.key.trim()) {
          headers[header.key.trim()] = header.value || "";
        }
      }
    }

    return headers;
  }

  /**
   * Apply a custom request body
   *
   * Merge custom request body configurations
   *
   * @param baseBody: The base request body
   * @param config: The format converter configuration
   * @returns: The merged request body
   */
  protected applyCustomBody(baseBody: unknown, config: FormatterConfig): unknown {
    // If the custom request body is not enabled, return it directly.
    if (config.customBodyEnabled === false) {
      return baseBody;
    }

    let result = { ...(baseBody as Record<string, unknown>) };

    // Handling a simplified version of a custom request body
    if (config.customBody) {
      result = this.deepMerge(result, config.customBody) as Record<string, unknown>;
    }

    // Process the complete custom request body.
    if (config.customBodyConfig) {
      const customBody = config.customBodyConfig;

      if (customBody.mode === "simple" && customBody.items) {
        // Simple mode: Iterate through all items
        for (const item of customBody.items) {
          if (item.enabled === false || !item.key || !item.key.trim()) {
            continue;
          }

          const rawKey = item.key.trim();
          let value: unknown;

          // Try to parse the value as JSON.
          try {
            value = JSON.parse(item.value);
          } catch {
            // Parse failed. Use the original string.
            value = item.value;
          }

          // 处理嵌套路径键名(如 "extra_body.google")
          if (rawKey.includes(".")) {
            const parts = rawKey.split(".");
            const nestedObj: Record<string, unknown> = {};
            let current: Record<string, unknown> = nestedObj;
            for (let i = 0; i < parts.length - 1; i++) {
              const part = parts[i]!;
              current[part] = {};
              current = current[part] as Record<string, unknown>;
            }
            const lastPart = parts[parts.length - 1]!;
            current[lastPart] = value;
            result = this.deepMerge(result, nestedObj) as Record<string, unknown>;
          } else {
            result = this.deepMerge(result, { [rawKey]: value }) as Record<string, unknown>;
          }
        }
      } else if (customBody.mode === "advanced" && customBody.json) {
        // Advanced mode: Parse the complete JSON and perform deep merging.
        try {
          const customData = JSON.parse(customBody.json);
          result = this.deepMerge(result, customData) as Record<string, unknown>;
        } catch (error) {
          logger.warn("Failed to parse custom body JSON", { error: getErrorOrNew(error) });
        }
      }
    }

    return result;
  }

  /**
   * Construct a query parameter string
   *
   * @param config Format converter configuration
   * @returns Query parameter string (with a ? prefix, if there are parameters)
   */
  protected buildQueryString(config: FormatterConfig): string {
    if (!config.queryParams || Object.keys(config.queryParams).length === 0) {
      return "";
    }

    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(config.queryParams)) {
      params.append(key, String(value));
    }

    return `?${params.toString()}`;
  }

  /**
   * Construct streaming options
   *
   * @param config Format converter configuration
   * @returns Streaming options object
   */
  protected buildStreamOptions(config: FormatterConfig): Record<string, unknown> | undefined {
    if (!config.streamOptions) {
      return undefined;
    }

    const options: Record<string, unknown> = {};

    if (config.streamOptions.includeUsage) {
      options["include_usage"] = true;
    }

    return Object.keys(options).length > 0 ? options : undefined;
  }

  // ==================== Multi-format Tool Invocation Parsing Method (Delegated to ToolCallParser) ====================

  /**
   * Parse tool calls from XML format text
   *
   * Supported formats:
   * ```xml
   * <tool_use>
   * <tool_name>tool_name</tool_name>
   * <parameters>
   * <param1>value1</param1>
   * <param2>value2</param2>
   * </parameters>
   * </tool_use>
   * ```
   *
   * @param xmlText: The XML text containing the tool calls
   * @returns: An array of parsed tool calls, converted to the standard LLMToolCall format
   */
  parseXMLToolCalls(xmlText: string): LLMToolCall[] {
    return ToolCallParser.parseXMLToolCalls(xmlText);
  }

  /**
   * Parse tool calls from JSON format text
   *
   * Supported formats:
   * ```
   * <<<TOOL_CALL>>>
   * {"tool": "tool_name", "parameters": {...}}
   * <<<END_TOOL_CALL>>>
   * ```
   *
   * @param text: Text containing JSON tool calls
   * @param options: Parsing options
   * @returns: Array of parsed tool calls (converted to the standard LLMToolCall format)
   */
  parseJSONToolCalls(text: string, options?: ToolCallParseOptions): LLMToolCall[] {
    return ToolCallParser.parseJSONToolCalls(text, options);
  }

  /**
   * Attempt to parse tool calls from the text (automatically detect the format).
   *
   * Try in the following order:
   * 1. XML format
   * 2. JSON format
   *
   * @param text: The text containing the tool calls
   * @param options: Parsing options
   * @returns: An array of the parsed tool calls
   */
  parseToolCallsFromText(text: string, options?: ToolCallParseOptions): LLMToolCall[] {
    return ToolCallParser.parseFromText(text, options);
  }
}
