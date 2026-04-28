/**
 * LLM Event Builders
 * Provides builders for LLM-related events
 */

import { createBuilder } from "./common.js";
import type {
  MessageAddedEvent,
  TokenUsageWarningEvent,
  ConversationStateChangedEvent,
  TokenLimitExceededEvent,
  ContextCompressionRequestedEvent,
  ContextCompressionCompletedEvent,
  LLMStreamAbortedEvent,
  LLMStreamErrorEvent,
} from "@wf-agent/types";

// =============================================================================
// Message Events
// =============================================================================

/**
 * Build message added event
 */
export const buildMessageAddedEvent = createBuilder<MessageAddedEvent>("MESSAGE_ADDED");

// =============================================================================
// Token Events
// =============================================================================

/**
 * Build token usage warning event
 */
export const buildTokenUsageWarningEvent =
  createBuilder<TokenUsageWarningEvent>("TOKEN_USAGE_WARNING");

/**
 * Build token limit exceeded event
 */
export const buildTokenLimitExceededEvent =
  createBuilder<TokenLimitExceededEvent>("TOKEN_LIMIT_EXCEEDED");

// =============================================================================
// Conversation Events
// =============================================================================

/**
 * Build conversation state changed event
 */
export const buildConversationStateChangedEvent = createBuilder<ConversationStateChangedEvent>(
  "CONVERSATION_STATE_CHANGED",
);

// =============================================================================
// Context Compression Events
// =============================================================================

/**
 * Build context compression requested event
 */
export const buildContextCompressionRequestedEvent =
  createBuilder<ContextCompressionRequestedEvent>("CONTEXT_COMPRESSION_REQUESTED");

/**
 * Build context compression completed event
 */
export const buildContextCompressionCompletedEvent =
  createBuilder<ContextCompressionCompletedEvent>("CONTEXT_COMPRESSION_COMPLETED");

// =============================================================================
// LLM Stream Events
// =============================================================================

/**
 * Build LLM stream aborted event
 */
export const buildLLMStreamAbortedEvent =
  createBuilder<LLMStreamAbortedEvent>("LLM_STREAM_ABORTED");

/**
 * Build LLM stream error event
 */
export const buildLLMStreamErrorEvent = createBuilder<LLMStreamErrorEvent>("LLM_STREAM_ERROR");
