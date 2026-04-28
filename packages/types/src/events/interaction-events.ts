/**
 * Interaction-related event type definitions
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";
import type { UserInteractionOperationType } from "../interaction.js";

/**
 * User interaction requested event type
 */
export interface UserInteractionRequestedEvent extends BaseEvent {
  type: "USER_INTERACTION_REQUESTED";
  /** Interaction ID */
  interactionId: ID;
  /** Operation type */
  operationType: UserInteractionOperationType;
  /** Prompt message */
  prompt: string;
  /** Timeout in milliseconds */
  timeout: number;
  /** Additional context data (optional) */
  contextData?: Record<string, unknown>;
}

/**
 * User interaction response event types
 */
export interface UserInteractionRespondedEvent extends BaseEvent {
  type: "USER_INTERACTION_RESPONDED";
  /** Interaction ID */
  interactionId: ID;
  /** User input data */
  inputData: unknown;
}

/**
 * User Interaction Processing Completion Event Type
 */
export interface UserInteractionProcessedEvent extends BaseEvent {
  type: "USER_INTERACTION_PROCESSED";
  /** Interaction ID */
  interactionId: ID;
  /** Type of operation */
  operationType: string;
  /** Disposal results */
  results: unknown;
}

/**
 * User interaction failure event types
 */
export interface UserInteractionFailedEvent extends BaseEvent {
  type: "USER_INTERACTION_FAILED";
  /** Interaction ID */
  interactionId: ID;
  /** Reasons for failure */
  reason: string;
}

/**
 * HumanRelay requested event type
 */
export interface HumanRelayRequestedEvent extends BaseEvent {
  type: "HUMAN_RELAY_REQUESTED";
  /** Request ID */
  requestId: ID;
  /** Prompt message */
  prompt: string;
  /** Message count */
  messageCount: number;
  /** Timeout in milliseconds */
  timeout: number;
}

/**
 * HumanRelay Response Event Type
 */
export interface HumanRelayRespondedEvent extends BaseEvent {
  type: "HUMAN_RELAY_RESPONDED";
  /** Request ID */
  requestId: ID;
  /** Manual input of content */
  content: string;
}

/**
 * HumanRelay Processing Completion Event Type
 */
export interface HumanRelayProcessedEvent extends BaseEvent {
  type: "HUMAN_RELAY_PROCESSED";
  /** Request ID */
  requestId: ID;
  /** Disposal results */
  message: {
    role: string;
    content: string;
  };
  /** Execution time (milliseconds) */
  executionTime: number;
}

/**
 * HumanRelay Failure Event Type
 */
export interface HumanRelayFailedEvent extends BaseEvent {
  type: "HUMAN_RELAY_FAILED";
  /** Request ID */
  requestId: ID;
  /** Reasons for failure */
  reason: string;
}
