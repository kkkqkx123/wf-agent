/**
 * Token Encoder Tool
 * Based on TokenEstimator for fast token counting, supports streaming responses, multimodal content, and tool calls
 *
 * Features:
 * - StreamingTokenCounter: Supports streaming incremental counting
 * - Handles various message types such as text, images, tool calls, and thought content
 * - Provides detailed token statistics
 */

import { estimateTokens } from "./token-estimator.js";
import type { MessageContent } from "@wf-agent/types";

/**
 * Incremental token counter for streaming responses
 * Tracks text, inference content, and tool calls to provide accurate token estimates.
 */
export class StreamingTokenCounter {
  private accumulatedText: string = "";
  private accumulatedReasoning: string = "";
  private toolCalls: Map<string, { name: string; args: string }> = new Map();
  private textTokenCount: number = 0;
  private reasoningTokenCount: number = 0;
  private toolCallsTokenCount: number = 0;

  /**
   * Add text content and return the increment token count
   * @param text - The new text to be added
   * @returns - The number of tokens in the added text
   */
  addText(text: string): number {
    if (!text || text.length === 0) {
      return 0;
    }

    this.accumulatedText += text;
    const newTotalTokens = estimateTokens(this.accumulatedText);
    const incrementalTokens = newTotalTokens - this.textTokenCount;
    this.textTokenCount = newTotalTokens;

    return incrementalTokens;
  }

  /**
   * Add reasoning content and return the increment of token count
   * @param text - The new reasoning text
   * @returns - The number of tokens in the new reasoning text
   */
  addReasoning(text: string): number {
    if (!text || text.length === 0) {
      return 0;
    }

    this.accumulatedReasoning += text;
    const newTotalTokens = estimateTokens(this.accumulatedReasoning);
    const incrementalTokens = newTotalTokens - this.reasoningTokenCount;
    this.reasoningTokenCount = newTotalTokens;

    return incrementalTokens;
  }

  /**
   * Add or update a tool call and return the incremental token count.
   * @param toolCallId - A unique identifier for the tool call (used to distinguish multiple calls of the same tool)
   * @param toolName - The name of the tool
   * @param args - Tool parameters (partial or complete)
   * @returns The incremental token count for this tool call
   */
  addToolCall(toolCallId: string, toolName: string, args: string): number {
    if (!toolCallId || !toolName) {
      return 0;
    }

    // Find existing tool calls by ID (multiple calls to the same tool are supported).
    const existingCall = this.toolCalls.get(toolCallId);
    const toolCallStr = `Tool: ${toolName}\nArguments: ${args}`;
    const newTokens = estimateTokens(toolCallStr);

    if (existingCall) {
      // Update the streaming parameters of the existing tool calls.
      const oldToolCallStr = `Tool: ${existingCall.name}\nArguments: ${existingCall.args}`;
      const oldTokens = estimateTokens(oldToolCallStr);
      this.toolCallsTokenCount -= oldTokens;

      this.toolCalls.set(toolCallId, { name: toolName, args });
      this.toolCallsTokenCount += newTokens;

      return newTokens - oldTokens;
    } else {
      // Add a new tool call
      this.toolCalls.set(toolCallId, { name: toolName, args });
      this.toolCallsTokenCount += newTokens;
      return newTokens;
    }
  }

  /**
   * Get the total token count for all accumulated content
   * @returns Total token count (text + inference + tool calls)
   */
  getTotalTokens(): number {
    return this.textTokenCount + this.reasoningTokenCount + this.toolCallsTokenCount;
  }

  /**
   * Get token count details by category
   * Used for debugging and understanding token distribution
   * @returns An object containing text, reasoning, toolCalls, and total counts
   */
  getTokenBreakdown(): {
    text: number;
    reasoning: number;
    toolCalls: number;
    total: number;
  } {
    return {
      text: this.textTokenCount,
      reasoning: this.reasoningTokenCount,
      toolCalls: this.toolCallsTokenCount,
      total: this.getTotalTokens(),
    };
  }

  /**
   * Reset the counter
   */
  reset(): void {
    this.accumulatedText = "";
    this.accumulatedReasoning = "";
    this.toolCalls = new Map();
    this.textTokenCount = 0;
    this.reasoningTokenCount = 0;
    this.toolCallsTokenCount = 0;
  }
}

/**
 * Serialize the `tool_use` block into text for token counting. How do similar APIs view tool calls?
 */
function serializeToolUse(toolUse: {
  id: string;
  name: string;
  input: Record<string, unknown> | string;
}): string {
  const parts = [`Tool: ${toolUse.name}`];
  if (toolUse.input !== undefined) {
    try {
      const inputStr =
        typeof toolUse.input === "string" ? toolUse.input : JSON.stringify(toolUse.input);
      parts.push(`Arguments: ${inputStr}`);
    } catch {
      parts.push(`Arguments: [serialization error]`);
    }
  }
  return parts.join("\n");
}

/**
 * Serialize the `tool_result` block into text for token counting.
 * Process string content and array content.
 */
function serializeToolResult(toolResult: {
  tool_use_id: string;
  content: string | Array<{ type: string; text: string }>;
}): string {
  const parts = [`Tool Result (${toolResult.tool_use_id})`];

  const content = toolResult.content;
  if (typeof content === "string") {
    parts.push(content);
  } else if (Array.isArray(content)) {
    // Recursively process an array of content blocks.
    for (const item of content) {
      if (item.type === "text") {
        parts.push(item.text || "");
      } else if (item.type === "image") {
        parts.push("[Image content]");
      } else {
        parts.push(`[Unsupported content block: ${String(item.type)}]`);
      }
    }
  }

  return parts.join("\n");
}

/**
 * Estimate the token count of an image
 */
function estimateImageTokens(imageUrl: { url: string }): number {
  const url = imageUrl.url;

  // Check if it is an image encoded in base64.
  if (url.startsWith("data:image/")) {
    try {
      // Extract the base64 data portion (remove the data:image/xxx;base64 prefix).
      const base64Data = url.split(",")[1];
      if (!base64Data) {
        return 170; // Conservative estimate
      }

      // Base64 encoding: 4 characters = 3 bytes, actual data size = base64.length * 3/4
      const estimatedDataBytes = base64Data.length * 0.75;

      // Estimated resolution from data size (assuming typical JPEG compression of ~0.5 bytes/pixel)
      // Pixel ≈ dataBytes / 0.5 = dataBytes * 2
      // Vision tokens ≈ ceil(pixels / 750) + 200
      const estimatedPixels = estimatedDataBytes * 2;
      const estimatedTokens = Math.ceil(estimatedPixels / 750) + 200;

      return estimatedTokens;
    } catch {
      return 170; // Use a conservative estimate in case of parsing failure.
    }
  }

  // For URL images, use a conservative estimate
  // Standard estimate: A typical image (VGA 640x480) contains approximately 170 tokens.
  return 170;
}

/**
 * Count the number of tokens in the message content
 * Supports text, images, tool calls, tool results, and thought content
 *
 * @param content - Message content (either a string or an array of content blocks)
 * @returns - The number of tokens
 */
export function countMessageTokens(content: MessageContent): number {
  if (!content) {
    return 0;
  }

  // If it's a string, encode it directly.
  if (typeof content === "string") {
    return estimateTokens(content);
  }

  // If it's an array, process each content block.
  if (Array.isArray(content)) {
    let totalTokens = 0;

    for (const block of content) {
      if (block.type === "text") {
        const text = block.text || "";
        if (text.length > 0) {
          totalTokens += estimateTokens(text);
        }
      } else if (block.type === "image_url") {
        if (block.image_url) {
          totalTokens += estimateImageTokens(block.image_url);
        } else {
          totalTokens += 170; // Conservative estimate
        }
      } else if (block.type === "tool_use") {
        if (block.tool_use) {
          const serialized = serializeToolUse(block.tool_use);
          if (serialized.length > 0) {
            totalTokens += estimateTokens(serialized);
          }
        }
      } else if (block.type === "tool_result") {
        if (block.tool_result) {
          const serialized = serializeToolResult(block.tool_result);
          if (serialized.length > 0) {
            totalTokens += estimateTokens(serialized);
          }
        }
      } else if (block.type === "thinking") {
        const thinking = block.thinking || "";
        if (thinking.length > 0) {
          totalTokens += estimateTokens(thinking);
        }
      }
    }

    return totalTokens;
  }

  return 0;
}

/**
 * Estimate the token count of text
 */
export function encodeText(text: string): number {
  return estimateTokens(text);
}

/**
 * Estimate the token count of an object
 * Serialize to JSON, then estimate
 *
 * @param obj - The object
 * @returns The number of tokens
 */
export function encodeObject(obj: unknown): number {
  try {
    return estimateTokens(JSON.stringify(obj));
  } catch {
    return estimateTokens(String(obj));
  }
}
