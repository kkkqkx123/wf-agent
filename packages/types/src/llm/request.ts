/**
 * LLM Request Type Definition
 */

import type { ID } from "../common.js";
import type { Message } from "../message/index.js";
import type { ToolSchema } from "../tool/index.js";
import type { ToolCallFormatConfig } from "./tool-call-format.js";
import type { ToolCallProtocolViolationPolicy } from "./protocol-config.js";

/**
 * Dead loop detection configuration for LLM requests
 */
export interface DeadLoopDetectionConfig {
  /** Enable dead loop detection (default: true) */
  enabled?: boolean;
  /** Checkpoint thresholds (character count) */
  checkpoints?: number[];
  /** Short sequence detection window size (characters) */
  shortSequenceWindow?: number;
  /** Minimum repeat unit length (characters) */
  minRepeatUnitLength?: number;
  /** Minimum repeat count */
  minRepeatCount?: number;
  /** Minimum period elements (blocks/lines) */
  minPeriodElements?: number;
  /** Maximum period length */
  maxPeriodLength?: number;
}

/**
 * LLM Request Type
 */
export interface LLMRequest {
  /** Referenced LLM Profile ID (optional, default configuration used if not provided) */
  profileId?: ID;
  /** message array */
  messages: Message[];
  /** Request Parameters object (overrides parameters in Profile) */
  parameters?: Record<string, unknown>;
  /** Definition of available tools */
  tools?: ToolSchema[];
  /**
   * Tool call format configuration override.
   *
   * @deprecated This field is currently not used in the execution path.
   * Use `lockedToolCallFormat` instead, which is set by the executor at execution start
   * and provides protocol locking and enforcement. The `toolCallFormat` field on
   * `LLMProfile` is the source of truth for the profile-level format.
   * If you need per-request overrides, set the profile's toolCallFormat or use
   * the AgentLoopDefinition's toolCallFormat.
   */
  toolCallFormat?: ToolCallFormatConfig;

  /**
   * Locked tool call format for this execution.
   * Set by the executor at execution start.
   * Overrides profile-level toolCallFormat.
   * The LLMClient must use this format regardless of profile changes.
   */
  lockedToolCallFormat?: ToolCallFormatConfig;

  /**
   * Execution ID for tracing and correlation.
   * Set by the executor to correlate log entries and metrics
   * across the full LLM call chain.
   */
  executionId?: string;

  /**
   * Per-request protocol violation policy override.
   * When set, overrides the global default policy for this request only.
   * Resolution order: request-level > agent-level > global default.
   */
  violationPolicy?: ToolCallProtocolViolationPolicy;

  /** Streaming or not */
  stream?: boolean;
  /** AbortSignal for interrupt requests */
  signal?: AbortSignal;
  /** Dead loop detection configuration */
  deadLoopDetection?: DeadLoopDetectionConfig;
}
