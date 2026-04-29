/**
 * History Converter
 *
 * Converts message history between native function-calling format
 * and text-based formats (XML/JSON).
 */

import type { LLMMessage, ToolCallFormat } from '@wf-agent/types';
import { ToolDeclarationFormatter } from '@wf-agent/prompt-templates';
import { DEFAULT_XML_TAGS, DEFAULT_JSON_MARKERS } from '@wf-agent/types';

export interface HistoryConversionOptions {
  /** Target format */
  targetFormat: ToolCallFormat;
  /** Custom XML tags */
  xmlTags?: typeof DEFAULT_XML_TAGS;
  /** Custom markers */
  markers?: typeof DEFAULT_JSON_MARKERS;
}

export class HistoryConverter {
  /**
   * Convert entire message history to text-based format
   */
  static convertToTextMode(
    messages: LLMMessage[],
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage[] {
    // No conversion needed for native mode
    if (format === 'function_call') {
      return messages;
    }

    return messages.map(msg => {
      if (msg.role === 'assistant' && msg.toolCalls?.length) {
        return this.convertAssistantMessage(msg, format, options);
      }
      if (msg.role === 'tool' && msg.toolCallId) {
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
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage {
    if (!message.toolCalls || message.toolCalls.length === 0) {
      return message;
    }

    const xmlTags = options?.xmlTags || DEFAULT_XML_TAGS;
    const markers = options?.markers || DEFAULT_JSON_MARKERS;

    const toolCallText = ToolDeclarationFormatter.formatToolCalls(
      message.toolCalls,
      {
        format: format === 'json_wrapped' || format === 'json_raw' ? 'json' : 'xml',
        xmlTags,
        markers,
      }
    );

    // Combine existing content with tool calls
    const combinedContent = message.content
      ? `${message.content}\n\n${toolCallText}`
      : toolCallText;

    // Return message without toolCalls field (they're now in content)
    return {
      role: 'assistant',
      content: combinedContent,
    };
  }

  /**
   * Convert tool result message to text format
   */
  static convertToolResultMessage(
    message: LLMMessage,
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage {
    const xmlTags = options?.xmlTags || DEFAULT_XML_TAGS;
    const markers = options?.markers || DEFAULT_JSON_MARKERS;

    const resultText = ToolDeclarationFormatter.formatToolResult(
      message,
      {
        format: format === 'json_wrapped' || format === 'json_raw' ? 'json' : 'xml',
        xmlTags,
        markers,
      }
    );

    // Tool results become user messages in text mode
    return {
      role: 'user',
      content: resultText,
    };
  }

  /**
   * Check if message needs conversion
   */
  static needsConversion(message: LLMMessage, targetFormat: ToolCallFormat): boolean {
    if (targetFormat === 'function_call') {
      return false;
    }

    // Assistant messages with toolCalls need conversion
    if (message.role === 'assistant' && message.toolCalls?.length) {
      return true;
    }

    // Tool messages need conversion
    if (message.role === 'tool' && message.toolCallId) {
      return true;
    }

    return false;
  }
}
