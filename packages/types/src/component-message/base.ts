/**
 * Base Component Message Types
 *
 * This module defines the foundational types for the component message system.
 * Component messages are used for output routing and multi-target delivery,
 * distinct from LLM messages (for LLM interaction) and Events (for internal communication).
 */

import type { EntityIdentity, MessageTrace } from "./entity.js";

/**
 * Component Message ID
 * Format: {category}:{entityType}:{entityId}:{timestamp}:{sequence}
 * Example: agent:loop:loop-001:1705312345678:42
 */
export type ComponentMessageId = string;

/**
 * Message Category - 8 major categories
 */
export const MessageCategory = {
  SYSTEM: "system",
  WORKFLOW_EXECUTION: "workflow_execution",
  AGENT: "agent",
  TOOL: "tool",
  HUMAN_RELAY: "human_relay",
  SUBGRAPH: "subgraph",
  CHECKPOINT: "checkpoint",
  EVENT: "event",
} as const;

/**
 * Message Category Type
 */
export type MessageCategory = typeof MessageCategory[keyof typeof MessageCategory];

/**
 * Message Level
 */
export type MessageLevel = "debug" | "info" | "warn" | "error" | "critical";

/**
 * Base Component Message Interface
 * All component messages must implement this interface.
 */
export interface BaseComponentMessage {
  /** Unique message identifier */
  readonly id: ComponentMessageId;

  /** Message category */
  readonly category: MessageCategory;

  /** Message type (defined within each category) */
  readonly type: string;

  /** Timestamp (milliseconds since epoch) */
  readonly timestamp: number;

  /** Message level */
  readonly level: MessageLevel;

  /** Entity identity (source of the message) */
  readonly entity: EntityIdentity;

  /** Message payload data */
  readonly data: unknown;

  /** Trace information for debugging */
  readonly trace?: MessageTrace;
}

/**
 * Message creation input (without auto-generated fields)
 */
export type CreateMessageInput = Omit<BaseComponentMessage, "id" | "timestamp">;
