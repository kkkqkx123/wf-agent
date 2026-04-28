/**
 * Message Builder
 * Provides a unified interface for message construction
 */

import type { LLMMessage, LLMToolCall } from "@wf-agent/types";
import type { ToolExecutionResult } from "@wf-agent/types";

/**
 * Message Builder Class
 *
 * Responsibilities:
 * - Unify the message construction interface
 * - Provide methods for constructing various types of messages
 */
export class MessageBuilder {
  /**
   * Construct a user message
   * @param content: The content of the message
   * @returns: The user message
   */
  static buildUserMessage(content: string): LLMMessage {
    return {
      role: "user",
      content,
    };
  }

  /**
   * Build Assistant Message
   * @param content Message content
   * @param toolCalls Array of tool calls (optional)
   * @param thinking Thinking content (optional)
   * @returns Assistant message
   */
  static buildAssistantMessage(
    content: string,
    toolCalls?: LLMToolCall[],
    thinking?: string,
  ): LLMMessage {
    const message: LLMMessage = {
      role: "assistant",
      content,
    };

    if (toolCalls && toolCalls.length > 0) {
      message.toolCalls = toolCalls;
    }

    if (thinking) {
      message.thinking = thinking;
    }

    return message;
  }

  /**
   * Build tool result message
   * @param toolCallId: Tool call ID
   * @param result: Tool execution result
   * @returns: Tool result message
   */
  static buildToolMessage(toolCallId: string, result: ToolExecutionResult): LLMMessage {
    const content = result.success
      ? JSON.stringify(result.result)
      : JSON.stringify({ error: result.error });

    return {
      role: "tool",
      content,
      toolCallId,
    };
  }

  /**
   * Constructing system messages
   * @param content: Message content
   * @returns: System message
   */
  static buildSystemMessage(content: string): LLMMessage {
    return {
      role: "system",
      content,
    };
  }

  /**
   * Build tool description message
   * @param descriptionText: The text of the tool description
   * @returns: The tool description message; returns null if the description is empty
   */
  static buildToolDescriptionMessage(descriptionText: string): LLMMessage | null {
    if (!descriptionText) {
      return null;
    }

    return {
      role: "system",
      content: descriptionText,
    };
  }
}
