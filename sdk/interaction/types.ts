/**
 * Interaction Module Types
 */

import type { BaseEvent } from "@wf-agent/types";

/**
 * Standard response format for interaction handlers
 */
export interface InteractionResponse {
  /** The data provided by the user */
  data: Record<string, unknown>;
  /** Optional additional information */
  additionalInfo?: string;
}

/**
 * Generic Interaction Handler Interface
 * Defines the contract for handling specific interaction types
 */
export interface IInteractionHandler<T extends BaseEvent = BaseEvent> {
  /**
   * The event type this handler is responsible for
   */
  readonly eventType: string;

  /**
   * Render the UI and collect user input
   * @param event The request event containing details
   * @returns The collected response data
   */
  handle(event: T): Promise<InteractionResponse>;
}
