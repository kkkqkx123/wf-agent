/**
 * Cross-Boundary Protocol Converter
 *
 * Converts conversation history and tool call format between different
 * execution contexts with potentially different protocols.
 *
 * Used when execution crosses boundaries:
 * - Sub-agent spawn (AgentLoopEntity → AgentLoopEntity)
 * - Workflow fork (WorkflowExecution → WorkflowExecution)
 * - Triggered sub-workflow (AgentLoopEntity → WorkflowExecution)
 * - Workflow to Agent (WorkflowExecution → AgentLoopEntity)
 */

import type { LLMMessage, ToolCallFormatConfig } from "@wf-agent/types";
import { HistoryConverter } from "./history-converter.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "CrossBoundaryConverter" });

/**
 * Boundary type for cross-boundary conversion tracking
 */
export type BoundaryType =
  | "sub_agent"
  | "workflow_fork"
  | "triggered_subworkflow"
  | "workflow_to_agent";

/**
 * Conversion options for CrossBoundaryConverter
 */
export interface CrossBoundaryConversionOptions {
  /** Whether to strip tool call text from content after parsing (default: true) */
  stripToolCallText?: boolean;
  /** Whether to restore tool role from user messages (default: true) */
  restoreToolRole?: boolean;
}

/**
 * Cross-boundary protocol converter.
 *
 * Provides universal conversion functions used by all boundary types.
 * Converts messages between different tool call protocol formats
 * when execution crosses from one context to another.
 */
export class CrossBoundaryConverter {
  /**
   * Convert messages from source protocol to target protocol.
   * This is the universal conversion function used by all boundary types.
   *
   * Conversion strategy: always goes through "native" as the canonical intermediate
   * format to avoid N×N conversion complexity.
   *
   * @param messages Messages to convert
   * @param sourceFormat Source protocol format
   * @param targetFormat Target protocol format
   * @param options Conversion options
   * @returns Converted messages
   */
  static convert(
    messages: LLMMessage[],
    sourceFormat: ToolCallFormatConfig,
    targetFormat: ToolCallFormatConfig,
    options?: CrossBoundaryConversionOptions,
    boundaryType?: BoundaryType,
  ): LLMMessage[] {
    // If formats are the same, no conversion needed
    if (sourceFormat.format === targetFormat.format) {
      return messages;
    }

    logger.info("Cross-boundary protocol conversion", {
      from: sourceFormat.format,
      to: targetFormat.format,
      boundaryType: boundaryType || "unknown",
      messageCount: messages.length,
    });

    // Step 1: Convert from source format to canonical internal format (native)
    // Always go through "native" as the canonical intermediate
    const toCanonical = HistoryConverter.toNative(messages, sourceFormat, {
      stripToolCallText: options?.stripToolCallText,
      restoreToolRole: options?.restoreToolRole,
    });

    // Step 2: Convert from canonical to target format
    // If target is native, no further conversion needed
    if (targetFormat.format === "native") {
      logger.debug("Cross-boundary conversion complete (target is native)", {
        from: sourceFormat.format,
        messageCount: toCanonical.length,
      });
      return toCanonical;
    }

    const fromCanonical = HistoryConverter.fromNative(toCanonical, targetFormat);

    logger.debug("Cross-boundary conversion complete", {
      from: sourceFormat.format,
      to: targetFormat.format,
      messageCount: fromCanonical.length,
    });

    return fromCanonical;
  }

  /**
   * Convert a single LLM result (tool calls) from source to target format.
   * Used when passing execution results across boundaries.
   *
   * @param toolCalls Tool calls to convert
   * @param sourceFormat Source protocol format
   * @param targetFormat Target protocol format
   * @returns Converted tool calls
   */
  static convertToolCalls(
    toolCalls: Array<{ id: string; function: { name: string; arguments: string } }>,
    sourceFormat: ToolCallFormatConfig,
    targetFormat: ToolCallFormatConfig,
  ): Array<{ id: string; function: { name: string; arguments: string } }> {
    if (sourceFormat.format === targetFormat.format) {
      return toolCalls;
    }

    // Tool calls are already in a structured format (native-like).
    // When the target is a text-based format, the caller should use
    // convert() on the full message array instead.
    // For now, return tool calls as-is since the structured data
    // is handled by the HistoryConverter at the message level.
    return toolCalls;
  }
}