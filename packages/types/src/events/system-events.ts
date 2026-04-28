/**
 * System Event Type Definition
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";

/**
 * Token Exceeds Limit Event Type
 */
export interface TokenLimitExceededEvent extends BaseEvent {
  type: "TOKEN_LIMIT_EXCEEDED";
  /** Number of Token currently in use */
  tokensUsed: number;
  /** Token Limit Threshold */
  tokenLimit: number;
}

/**
 * Token uses the warning event type
 */
export interface TokenUsageWarningEvent extends BaseEvent {
  type: "TOKEN_USAGE_WARNING";
  /** Number of Token currently in use */
  tokensUsed: number;
  /** Token Limit Threshold */
  tokenLimit: number;
  /** Percentage of use */
  usagePercentage: number;
}

/**
 * Error Event Type
 */
export interface ErrorEvent extends BaseEvent {
  type: "ERROR";
  /** Node ID (optional) */
  nodeId?: ID;
  /** error message */
  error: unknown;
  /** stack trace */
  stackTrace?: string;
}

/**
 * Variable Change Event Type
 */
export interface VariableChangedEvent extends BaseEvent {
  type: "VARIABLE_CHANGED";
  /** variable name */
  variableName: string;
  /** variable value */
  variableValue: unknown;
  /** variable scope */
  variableScope: string;
}

/**
 * LLM Streaming Abort Event Type
 */
export interface LLMStreamAbortedEvent extends BaseEvent {
  type: "LLM_STREAM_ABORTED";
  /** Node ID (optional) */
  nodeId?: ID;
  /** Reason for discontinuation */
  reason: string;
}

/**
 * LLM Streaming Error Event Type
 */
export interface LLMStreamErrorEvent extends BaseEvent {
  type: "LLM_STREAM_ERROR";
  /** Node ID (optional) */
  nodeId?: ID;
  /** error message */
  error: string;
}

/**
 * Context Compression Request Event Type
 */
export interface ContextCompressionRequestedEvent extends BaseEvent {
  type: "CONTEXT_COMPRESSION_REQUESTED";
  /** Number of Token currently in use */
  tokensUsed: number;
  /** Token Limit Threshold */
  tokenLimit: number;
  /** Dialogue statistics */
  stats?: unknown;
}

/**
 * Context Compression Completion Event Type
 */
export interface ContextCompressionCompletedEvent extends BaseEvent {
  type: "CONTEXT_COMPRESSION_COMPLETED";
  /** Summary of compression results */
  summary?: string;
  /** Number of Token after compression (optional) */
  tokensAfter?: number;
}
