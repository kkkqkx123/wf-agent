/**
 * Conversation-related event type definitions
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";

/**
 * Message Add Event Type
 */
export interface MessageAddedEvent extends BaseEvent {
  type: "MESSAGE_ADDED";
  /** Node ID */
  nodeId?: ID;
  /** message role */
  role: string;
  /** Message content */
  content: string;
}

/**
 * Dialog state change event type
 */
export interface ConversationStateChangedEvent extends BaseEvent {
  type: "CONVERSATION_STATE_CHANGED";
  /** Node ID */
  nodeId?: ID;
  /** Number of messages */
  messageCount: number;
  /** Token usage */
  tokenUsage: number;
}
