/**
 * Conversation Event Builders
 * Provides builders for conversation-related events
 */

import { createBuilder } from "./common.js";
import type {
  MessageAddedEvent,
  ConversationStateChangedEvent,
} from "@wf-agent/types";

// =============================================================================
// Message Events
// =============================================================================

/**
 * Build message added event
 */
export const buildMessageAddedEvent = createBuilder<MessageAddedEvent>("MESSAGE_ADDED");

// =============================================================================
// Conversation State Events
// =============================================================================

/**
 * Build conversation state changed event
 */
export const buildConversationStateChangedEvent = createBuilder<ConversationStateChangedEvent>(
  "CONVERSATION_STATE_CHANGED",
);
