/**
 * Tool Call Parser
 *
 * Responsible for parsing tool calls from text in various formats (XML, JSON, raw JSON, native).
 * Operates independently of Formatters and can be reused by multiple Formatters.
 * Designed for:
 * - Small local LLMs without function calling support
 * - Web-based LLM debugging
 * - Cross-model compatibility fallback
 */

import type { LLMToolCall, ToolCallFormatMarkers } from "@wf-agent/types";
import { DEFAULT_JSON_MARKERS } from "@wf-agent/types";
import { sdkLogger as logger } from "../../../utils/logger.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import type { ToolCallParseOptions } from "./types.js";

/**
 * XML Tool Call Format
 */
export interface XMLToolCall {
  /** Tool Name */
  name: string;
  /** Tool parameters */
  args: Record<string, unknown>;
}

/**
 * JSON Tool Call Format
 */
export interface JSONToolCall {
  /** Tool name */
  tool: string;
  /** Tool parameters */
  parameters: Record<string, unknown>;
}

/**
 * Tool Call Parser Class
 *
 * Provides static methods for parsing tool calls in various formats:
 * - XML <tool_use>
 * - JSON wrapped with custom markers
 * - Raw JSON (single or array)
 * - Native OpenAI-style function calls
 */
export class ToolCallParser {
  /**
   * Parse tool calls from XML format text
   *
   * Supported format:
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
   * @param xmlText Text containing XML tool calls
   * @returns Array of parsed LLMToolCall objects
   */
  static parseXMLToolCalls(xmlText: string): LLMToolCall[] {
    const results: LLMToolCall[] = [];

    const toolUseRegex = /<tool_use>([\s\S]*?)<\/tool_use>/g;
    let match;

    while ((match = toolUseRegex.exec(xmlText)) !== null) {
      try {
        const content = match[1];
        if (!content) continue;

        const nameMatch = content.match(/<tool_name>([\s\S]*?)<\/tool_name>/);
        const toolName = nameMatch?.[1]?.trim();
        if (!toolName) continue;

        const paramsMatch = content.match(/<parameters>([\s\S]*?)<\/parameters>/);
        const args = paramsMatch?.[1] ? this.parseXMLParameters(paramsMatch[1]) : {};

        results.push({
          id: this.generateToolCallId(),
          type: "function",
          function: {
            name: toolName,
            arguments: JSON.stringify(args),
          },
        });
      } catch (error) {
        logger.warn("Failed to parse XML tool call block", { error: getErrorOrNew(error) });
      }
    }

    return results;
  }

  /**
   * Parse XML parameters with nested objects and arrays
   */
  private static parseXMLParameters(paramsContent: string): Record<string, unknown> {
    const result: Record<string, unknown> = {};
    const tagRegex = /<([\w_]+)>([\s\S]*?)<\/\1>/g;
    let match;

    while ((match = tagRegex.exec(paramsContent)) !== null) {
      const tag = match[1];
      const content = match[2]?.trim() ?? "";

      if (!tag) continue;

      if (/<[\w_]+>/.test(content)) {
        if (tag === "item" || content.includes("<item>")) {
          result[tag] = this.parseXMLArray(content);
        } else {
          result[tag] = this.parseXMLParameters(content);
        }
      } else {
        result[tag] = this.parseXMLValue(content);
      }
    }

    return result;
  }

  /**
   * Parse XML array using <item> elements
   */
  private static parseXMLArray(arrayContent: string): unknown[] {
    const items: unknown[] = [];
    const itemRegex = /<item>([\s\S]*?)<\/item>/g;
    let match;

    while ((match = itemRegex.exec(arrayContent)) !== null) {
      const content = match[1]?.trim() ?? "";
      if (/<[\w_]+>/.test(content)) {
        items.push(this.parseXMLParameters(content));
      } else {
        items.push(this.parseXMLValue(content));
      }
    }

    return items;
  }

  /**
   * Auto-convert XML string values to proper JS types
   */
  private static parseXMLValue(value: string): unknown {
    const trimmed = value.trim();
    if (/^-?\d+$/.test(trimmed)) return parseInt(trimmed, 10);
    if (/^-?\d+\.\d+$/.test(trimmed)) return parseFloat(trimmed);
    if (trimmed === "true") return true;
    if (trimmed === "false") return false;
    if (trimmed === "null") return null;
    return trimmed;
  }

  /**
   * Parse JSON tool calls wrapped with custom markers
   *
   * Default format:
   * <<<TOOL_CALL>>>
   * {"tool": "name", "parameters": {...}}
   * <<<END_TOOL_CALL>>>
   *
   * @param text Text containing wrapped JSON tool calls
   * @param options Parsing options including custom markers
   * @returns Array of parsed LLMToolCall objects
   */
  static parseJSONToolCalls(text: string, options: ToolCallParseOptions = {}): LLMToolCall[] {
    const results: LLMToolCall[] = [];

    // Use provided markers or fall back to defaults
    const markers: ToolCallFormatMarkers = options.markers || DEFAULT_JSON_MARKERS;
    const start = markers.start;
    const end = markers.end;

    const escapedStart = this.escapeRegExp(start);
    const escapedEnd = this.escapeRegExp(end);

    const regex = new RegExp(`${escapedStart}\\s*([\\s\\S]*?)\\s*${escapedEnd}`, "g");
    let match;

    while ((match = regex.exec(text)) !== null) {
      try {
        const jsonStr = match[1]?.trim();
        if (!jsonStr) continue;

        const parsed = JSON.parse(jsonStr);
        const toolCall = this.convertToStandardToolCall(parsed);
        if (toolCall) results.push(toolCall);
      } catch (error) {
        logger.warn("Failed to parse wrapped JSON tool call", { error: getErrorOrNew(error) });
      }
    }

    return results;
  }

  /**
   * Parse RAW JSON (no markers)
   * Supports:
   * - Single tool call object
   * - Array of tool call objects
   * - OpenAI native format { function: { name, arguments } }
   *
   * @param text Text containing raw JSON tool calls
   * @returns Array of parsed LLMToolCall objects
   */
  static parseRawJsonToolCalls(text: string): LLMToolCall[] {
    try {
      // Clean common LLM errors
      const cleaned = text
        .replace(/```json/g, "")
        .replace(/```/g, "")
        .trim();

      const parsed = JSON.parse(cleaned);
      const array = Array.isArray(parsed) ? parsed : [parsed];

      return array
        .map(item => this.convertToStandardToolCall(item))
        .filter(Boolean) as LLMToolCall[];
    } catch (e) {
      return [];
    }
  }

  /**
   * Parse partial tool calls (for streaming chunks)
   * Returns best-effort parsed tool calls even for incomplete JSON/XML
   *
   * @param text Text that may contain partial tool calls
   * @returns Array of parsed LLMToolCall objects
   */
  static parsePartial(text: string): LLMToolCall[] {
    if (!text) return [];

    // Try XML first
    if (text.includes("<tool_use>")) {
      return this.parseXMLToolCalls(text);
    }

    // Try wrapped JSON
    if (text.includes(DEFAULT_JSON_MARKERS.start)) {
      return this.parseJSONToolCalls(text, { allowPartial: true });
    }

    // Try raw JSON best-effort
    try {
      return this.parseRawJsonToolCalls(text);
    } catch {
      return [];
    }
  }

  /**
   * Convert any known tool call format to standard LLMToolCall
   *
   * @param parsed Parsed object from JSON
   * @returns Standard LLMToolCall or null if conversion fails
   */
  private static convertToStandardToolCall(parsed: unknown): LLMToolCall | null {
    try {
      if (!parsed || typeof parsed !== "object") return null;

      const obj = parsed as Record<string, unknown>;

      // Format 1: { tool: "name", parameters: {} }
      if (obj["tool"] && typeof obj["tool"] === "string") {
        return {
          id: this.generateToolCallId(),
          type: "function",
          function: {
            name: obj["tool"],
            arguments: JSON.stringify(obj["parameters"] || {}),
          },
        };
      }

      // Format 2: { name: "name", arguments: "{}" | {} }
      if (obj["name"] && typeof obj["name"] === "string") {
        const args =
          typeof obj["arguments"] === "string"
            ? obj["arguments"]
            : JSON.stringify(obj["arguments"] || {});

        return {
          id: this.generateToolCallId(),
          type: "function",
          function: {
            name: obj["name"],
            arguments: args,
          },
        };
      }

      // Format 3: OpenAI native { function: { name, arguments } }
      if (obj["function"] && typeof obj["function"] === "object") {
        const fn = obj["function"] as Record<string, unknown>;
        if (!fn["name"] || typeof fn["name"] !== "string") return null;

        const args =
          typeof fn["arguments"] === "string"
            ? fn["arguments"]
            : JSON.stringify(fn["arguments"] || {});

        return {
          id: (obj["id"] as string) || this.generateToolCallId(),
          type: "function",
          function: {
            name: fn["name"],
            arguments: args,
          },
        };
      }

      return null;
    } catch {
      return null;
    }
  }

  /**
   * Auto-detect and parse tool calls from text
   * Order: XML → Wrapped JSON → Raw JSON
   *
   * @param text Text that may contain tool calls
   * @param options Parsing options
   * @returns Array of parsed LLMToolCall objects
   */
  static parseFromText(text: string, options?: ToolCallParseOptions): LLMToolCall[] {
    if (!text || typeof text !== "string") return [];

    const preferred = options?.preferredFormats || ["xml", "json", "raw"];

    for (const format of preferred) {
      let calls: LLMToolCall[] = [];

      if (format === "xml") calls = this.parseXMLToolCalls(text);
      else if (format === "json") calls = this.parseJSONToolCalls(text, options);
      else if (format === "raw") calls = this.parseRawJsonToolCalls(text);

      if (calls.length > 0) return calls;
    }

    // Fallback for partial streaming content
    return options?.allowPartial ? this.parsePartial(text) : [];
  }

  /**
   * Generate a unique tool call ID compatible with OpenAI format
   *
   * @returns Unique tool call ID
   */
  private static generateToolCallId(): string {
    return `call_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
  }

  /**
   * Check if text contains XML tool calls
   *
   * @param text Text to check
   * @returns True if text contains XML tool calls
   */
  static hasXMLToolCalls(text: string): boolean {
    return text.includes("<tool_use>");
  }

  /**
   * Check if text contains wrapped JSON tool calls
   *
   * @param text Text to check
   * @returns True if text contains wrapped JSON tool calls
   */
  static hasJSONToolCalls(text: string): boolean {
    return text.includes(DEFAULT_JSON_MARKERS.start);
  }

  /**
   * Check if text contains raw JSON tool calls
   *
   * @param text Text to check
   * @returns True if text starts with { or [
   */
  static hasRawJsonToolCalls(text: string): boolean {
    const trimmed = text.trim();
    return trimmed.startsWith("{") || trimmed.startsWith("[");
  }

  /**
   * Escape special regex characters in a string
   *
   * @param str String to escape
   * @returns Escaped string safe for use in RegExp
   */
  static escapeRegExp(str: string): string {
    return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }
}

export default ToolCallParser;
