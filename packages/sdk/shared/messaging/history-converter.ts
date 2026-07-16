/**
 * History Converter
 *
 * Converts message history between native function-calling format
 * and text-based formats (XML/JSON).
 */

import type { LLMMessage, ToolCallFormat, ToolCallXmlTags, ToolCallFormatConfig, MessageContent } from "@wf-agent/types";
import { ToolDeclarationFormatter } from "../tools/index.js";
import { DEFAULT_XML_TAGS, DEFAULT_JSON_MARKERS } from "@wf-agent/types";
import { ToolCallParser } from "../../services/llm/formatters/tool-call-parser.js";
import { getToolCallParserOptions } from "../../services/llm/formatters/tool-format-selector.js";

/**
 * Extract string content from MessageContent.
 * If it's a string, return it directly. If it's an array, concatenate text parts.
 * Returns empty string for non-text content.
 */
function extractStringContent(content: MessageContent | undefined): string {
  if (!content) return "";
  if (typeof content === "string") return content;
  // Array of content blocks — extract text parts
  return content
    .filter((block): block is { type: "text"; text: string } => block.type === "text")
    .map(block => block.text)
    .join("\n");
}

export interface HistoryConversionOptions {
  /** Target format */
  targetFormat: ToolCallFormat;
  /** Custom XML tags */
  xmlTags?: ToolCallXmlTags;
  /** Custom markers */
  markers?: typeof DEFAULT_JSON_MARKERS;
}

/** Options for toNative() conversion */
export interface ToNativeOptions {
  /** Whether to remove tool call text from content after parsing (default: true) */
  stripToolCallText?: boolean;
  /** Whether to convert tool result user messages back to tool role (default: true) */
  restoreToolRole?: boolean;
}

export class HistoryConverter {
  /**
   * Convert entire message history to text-based format
   */
  static convertToTextMode(
    messages: LLMMessage[],
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>,
  ): LLMMessage[] {
    // No conversion needed for native mode
    if (format === "native") {
      return messages;
    }

    return messages.map(msg => {
      if (msg.role === "assistant" && msg.toolCalls?.length) {
        return this.convertAssistantMessage(msg, format, options);
      }
      if (msg.role === "tool" && msg.toolCallId) {
        return this.convertToolResultMessage(msg, format, options);
      }
      return msg;
    });
  }

  /**
   * Convert assistant message with tool calls to text format
   */
  static convertAssistantMessage(
    message: LLMMessage,
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>,
  ): LLMMessage {
    if (!message.toolCalls || message.toolCalls.length === 0) {
      return message;
    }

    const xmlTags = options?.xmlTags || DEFAULT_XML_TAGS;
    const markers = options?.markers || DEFAULT_JSON_MARKERS;

    const toolCallText = ToolDeclarationFormatter.formatToolCalls(message.toolCalls, {
      format: format === "json_wrapped" || format === "json_raw" ? "json" : "xml",
      xmlTags,
      markers,
    });

    // Combine existing content with tool calls
    const combinedContent = message.content
      ? `${message.content}\n\n${toolCallText}`
      : toolCallText;

    // Return message without toolCalls field (they're now in content)
    return {
      role: "assistant",
      content: combinedContent,
    };
  }

  /**
   * Convert tool result message to text format
   */
  static convertToolResultMessage(
    message: LLMMessage,
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>,
  ): LLMMessage {
    const xmlTags = options?.xmlTags || DEFAULT_XML_TAGS;
    const markers = options?.markers || DEFAULT_JSON_MARKERS;

    const resultText = ToolDeclarationFormatter.formatToolResult(
      message as unknown as { toolCallId?: string; content: string },
      {
        format: format === "json_wrapped" || format === "json_raw" ? "json" : "xml",
        xmlTags,
        markers,
      },
    );

    // Tool results become user messages in text mode
    return {
      role: "user",
      content: resultText,
    };
  }

  /**
   * Check if message needs conversion
   */
  static needsConversion(message: LLMMessage, targetFormat: ToolCallFormat): boolean {
    if (targetFormat === "native") {
      return false;
    }

    // Assistant messages with toolCalls need conversion
    if (message.role === "assistant" && message.toolCalls?.length) {
      return true;
    }

    // Tool messages need conversion
    if (message.role === "tool" && message.toolCallId) {
      return true;
    }

    return false;
  }

  /**
   * Convert text-mode messages back to native format.
   *
   * Reverses convertToTextMode() — parses tool calls from text content
   * back into structured toolCalls fields.
   *
   * Note: This is a lossy conversion because tool call IDs, structured arguments,
   * and role distinctions are embedded in free-form text. The heuristic parser
   * handles common cases but may not cover all edge cases.
   *
   * @param messages Messages to convert (in text format)
   * @param sourceFormat The source format configuration (currently used for format & markers)
   * @returns Converted messages with toolCalls and roles restored
   */
  static toNative(
    messages: LLMMessage[],
    sourceFormat: ToolCallFormatConfig,
    options?: ToNativeOptions,
  ): LLMMessage[] {
    const stripToolCallText = options?.stripToolCallText ?? true;
    const restoreToolRole = options?.restoreToolRole ?? true;

    // If already native, no conversion needed
    if (sourceFormat.format === "native") {
      return messages;
    }

    const parserOptions = getToolCallParserOptions(sourceFormat.format, sourceFormat.markers);

    return messages.map(msg => {
      // Convert assistant messages with embedded tool calls in content
      if (msg.role === "assistant" && msg.content && !msg.toolCalls) {
        const contentStr = extractStringContent(msg.content);
        const toolCalls = ToolCallParser.parseFromText(contentStr, parserOptions);

        if (toolCalls.length > 0) {
          const result: LLMMessage = {
            role: "assistant",
            content: stripToolCallText
              ? HistoryConverter.stripToolCallText(contentStr, sourceFormat.format, sourceFormat.markers)
              : contentStr,
            toolCalls: toolCalls.map(tc => ({
              id: tc.id,
              type: "function" as const,
              function: {
                name: tc.function.name,
                arguments: tc.function.arguments,
              },
            })),
          };
          return result;
        }
        return msg;
      }

      // Convert tool result messages that were converted to user role
      if (msg.role === "user" && msg.content && restoreToolRole) {
        const contentStr = extractStringContent(msg.content);
        const toolResult = HistoryConverter.parseToolResultFromText(contentStr, sourceFormat);
        if (toolResult) {
          return {
            role: "tool",
            toolCallId: toolResult.toolCallId,
            content: toolResult.content,
          };
        }
        return msg;
      }

      return msg;
    });
  }

  /**
   * Alias for convertToTextMode — converts native messages to text-mode format.
   * Provides semantic clarity when used alongside toNative().
   */
  static fromNative(
    messages: LLMMessage[],
    targetFormat: ToolCallFormatConfig,
  ): LLMMessage[] {
    return HistoryConverter.convertToTextMode(messages, targetFormat.format, {
      xmlTags: targetFormat.xmlTags,
      markers: targetFormat.markers as typeof DEFAULT_JSON_MARKERS | undefined,
    });
  }

  /**
   * Strip tool call text from assistant message content.
   * Removes the tool call representation (XML tags or JSON markers) from the content,
   * leaving only the natural language response.
   */
  private static stripToolCallText(
    content: string,
    format: ToolCallFormat,
    markers?: { start: string; end: string },
  ): string {
    if (format === "xml") {
      // Remove XML tool call blocks
      return content.replace(/<tool_use>[\s\S]*?<\/tool_use>/g, "").trim();
    }
    if (format === "json_wrapped" && markers) {
      // Remove JSON marker blocks
      const escapedStart = ToolCallParser.escapeRegExp(markers.start);
      const escapedEnd = ToolCallParser.escapeRegExp(markers.end);
      const regex = new RegExp(`${escapedStart}[\\s\\S]*?${escapedEnd}`, "g");
      return content.replace(regex, "").trim();
    }
    if (format === "json_raw") {
      // Remove raw JSON tool call blocks (heuristic: match {...} with "tool" or "function" key)
      return content.replace(/\{[^}]*"tool"[^}]*\}/g, "").trim();
    }
    return content.trim();
  }

  /**
   * Attempt to parse a tool result from a user message's text content.
   * Uses heuristic detection of tool result markers (XML tags or JSON markers).
   */
  private static parseToolResultFromText(
    content: string,
    format: ToolCallFormatConfig,
  ): { toolCallId: string; content: string } | null {
    if (format.format === "xml") {
      const xmlTags = format.xmlTags || DEFAULT_XML_TAGS;
      const toolResultRegex = new RegExp(
        `<${xmlTags.toolResult}>[\\s\\S]*?<${xmlTags.toolCallId}>([^<]+)</${xmlTags.toolCallId}>[\\s\\S]*?<${xmlTags.toolOutput}>([\\s\\S]*?)</${xmlTags.toolOutput}>[\\s\\S]*?</${xmlTags.toolResult}>`,
      );
      const match = content.match(toolResultRegex);
      if (match) {
        const toolCallId = match[1]?.trim() ?? "";
        const toolContent = match[2]?.trim() ?? "";
        return {
          toolCallId,
          content: toolContent,
        };
      }
      return null;
    }

    if (format.format === "json_wrapped" || format.format === "json_raw") {
      const markers = format.markers || DEFAULT_JSON_MARKERS;
      const escapedStart = ToolCallParser.escapeRegExp(markers.start);
      const escapedEnd = ToolCallParser.escapeRegExp(markers.end);
      const regex = new RegExp(
        `${escapedStart}\\s*\\{("tool_call_id"|"output")[^}]*\\}\\s*${escapedEnd}`,
        "g",
      );
      const match = content.match(regex);
      if (match) {
        try {
          const jsonStr = match[0]
            .replace(markers.start, "")
            .replace(markers.end, "")
            .trim();
          const parsed = JSON.parse(jsonStr);
          return {
            toolCallId: parsed.tool_call_id || "",
            content: parsed.output || parsed.content || "",
          };
        } catch {
          return null;
        }
      }
      return null;
    }

    return null;
  }
}
