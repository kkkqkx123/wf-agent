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
    // Use preserved original tool calls from metadata when available.
    // This provides lossless conversion: the original structured tool calls
    // (stored by toNative()) are preserved regardless of heuristic parsing.
    const metadata = message.metadata as Record<string, unknown> | undefined;
    const originalToolCalls = metadata?.["originalToolCalls"] as
      | Array<{ id: string; type: string; function: { name: string; arguments: string } }>
      | undefined;
    const toolCalls = originalToolCalls ?? message.toolCalls;

    if (!toolCalls || toolCalls.length === 0) {
      return message;
    }

    const xmlTags = options?.xmlTags || DEFAULT_XML_TAGS;
    const markers = options?.markers || DEFAULT_JSON_MARKERS;

    const toolCallText = ToolDeclarationFormatter.formatToolCalls(toolCalls, {
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
              ? HistoryConverter.stripToolCallText(contentStr, sourceFormat.format, sourceFormat.xmlTags, sourceFormat.markers)
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

          // Preserve original structured tool calls in metadata for lossless reverse conversion.
          // This allows fromNative() to reconstruct the exact tool calls when converting back,
          // avoiding information loss from the heuristic parsing of text-based formats.
          const originalToolCalls = result.toolCalls?.map(tc => ({
            id: tc.id,
            type: tc.type,
            function: {
              name: tc.function.name,
              arguments: tc.function.arguments,
            },
          }));

          if (originalToolCalls && originalToolCalls.length > 0) {
            result.metadata = {
              ...(result.metadata || {}),
              originalToolCalls,
            };
          }

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
    xmlTags?: ToolCallXmlTags,
    markers?: { start: string; end: string },
  ): string {
    if (format === "xml") {
      // Remove XML tool call blocks using configurable tag names
      const toolCallTag = xmlTags?.toolCall || "tool_use";
      const regex = new RegExp(`<${toolCallTag}>[\\s\\S]*?<\\/${toolCallTag}>`, "g");
      return content.replace(regex, "").trim();
    }
    if (format === "json_wrapped" && markers) {
      // Remove JSON marker blocks
      const escapedStart = ToolCallParser.escapeRegExp(markers.start);
      const escapedEnd = ToolCallParser.escapeRegExp(markers.end);
      const regex = new RegExp(`${escapedStart}[\\s\\S]*?${escapedEnd}`, "g");
      return content.replace(regex, "").trim();
    }
    if (format === "json_raw") {
      // Remove raw JSON tool call blocks.
      // Heuristic: match JSON objects containing tool call identifiers (tool, function, name keys).
      // Uses a simple balanced-brace approach to handle single-level nesting.
      return HistoryConverter.removeRawJsonToolCalls(content);
    }
    return content.trim();
  }

  /**
   * Remove raw JSON tool call objects from text using heuristic detection.
   * Handles single-level nesting of JSON objects.
   */
  private static removeRawJsonToolCalls(content: string): string {
    const toolCallKeyPattern = /"tool"\s*:|"function"\s*:|"name"\s*:/;
    const result: string[] = [];
    let i = 0;
    while (i < content.length) {
      const braceStart = content.indexOf("{", i);
      if (braceStart === -1) {
        // No more JSON objects, append remaining text
        result.push(content.slice(i));
        break;
      }
      // Try to find the matching closing brace with nesting support
      let depth = 0;
      let braceEnd = -1;
      for (let j = braceStart; j < content.length; j++) {
        if (content[j] === "{") {
          depth++;
        } else if (content[j] === "}") {
          depth--;
          if (depth === 0) {
            braceEnd = j;
            break;
          }
        }
      }
      if (braceEnd === -1) {
        // Unmatched brace, append remaining text
        result.push(content.slice(i));
        break;
      }
      const candidate = content.slice(braceStart, braceEnd + 1);
      // Check if this JSON object looks like a tool call
      if (toolCallKeyPattern.test(candidate)) {
        // It's a tool call — skip it (strip)
        // Append text before the brace
        result.push(content.slice(i, braceStart));
        i = braceEnd + 1;
      } else {
        // Not a tool call — keep it
        result.push(content.slice(i, braceEnd + 1));
        i = braceEnd + 1;
      }
    }
    return result.join("").trim();
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

    if (format.format === "json_wrapped") {
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

    if (format.format === "json_raw") {
      // json_raw has no markers — look for raw JSON objects containing tool result keys
      return HistoryConverter.parseRawJsonToolResult(content);
    }

    return null;
  }

  /**
   * Parse a raw JSON tool result from text (no markers).
   * Searches for JSON objects containing tool_call_id and output/content keys.
   * Uses balanced-brace matching to handle nested objects.
   */
  private static parseRawJsonToolResult(content: string): { toolCallId: string; content: string } | null {
    const toolResultKeyPattern = /"tool_call_id"\s*:/;
    let i = 0;
    while (i < content.length) {
      const braceStart = content.indexOf("{", i);
      if (braceStart === -1) {
        return null;
      }
      // Find matching closing brace with nesting support
      let depth = 0;
      let braceEnd = -1;
      for (let j = braceStart; j < content.length; j++) {
        if (content[j] === "{") {
          depth++;
        } else if (content[j] === "}") {
          depth--;
          if (depth === 0) {
            braceEnd = j;
            break;
          }
        }
      }
      if (braceEnd === -1) {
        return null;
      }
      const candidate = content.slice(braceStart, braceEnd + 1);
      if (toolResultKeyPattern.test(candidate)) {
        try {
          const parsed = JSON.parse(candidate);
          return {
            toolCallId: parsed.tool_call_id || "",
            content: parsed.output || parsed.content || "",
          };
        } catch {
          // Invalid JSON, try next object
          i = braceEnd + 1;
          continue;
        }
      }
      i = braceEnd + 1;
    }
    return null;
  }
}
