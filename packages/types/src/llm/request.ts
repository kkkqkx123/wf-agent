/**
 * LLM Request Type Definition
 */

import type { ID } from "../common.js";
import type { Message } from "../message/index.js";
import type { ToolSchema } from "../tool/index.js";
import type { ToolCallFormatConfig } from "./tool-call-format.js";

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
   * Overrides the profile-level toolCallFormat for this specific request.
   */
  toolCallFormat?: ToolCallFormatConfig;
  /** Streaming or not */
  stream?: boolean;
  /** AbortSignal for interrupt requests */
  signal?: AbortSignal;
  /** Dead loop detection configuration */
  deadLoopDetection?: DeadLoopDetectionConfig;
}
