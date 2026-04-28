/**
 * OnceEventSubscription - Register One-time Agent Event Listener
 *
 * Responsibilities:
 * - Encapsulates one-time event listener registration as Subscription pattern
 * - Provides unified API layer interface for Agent events
 * - Auto-unsubscribes after first event
 *
 * Design Principles:
 * - Follows Subscription pattern, inherits BaseSubscription
 * - Uses dependency injection for EventRegistry
 * - Returns unsubscribe function
 */

import { BaseSubscription, SubscriptionMetadata } from "../../../../shared/types/subscription.js";
import type { EventType, EventListener, BaseEvent } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../../shared/core/sdk-dependencies.js";

/**
 * Register one-time event listener parameters
 */
export interface OnceAgentEventParams {
  /** Event Type */
  eventType: EventType;
  /** Event listener */
  listener: EventListener<BaseEvent>;
}

/**
 * OnceEventSubscription - Registers a one-time event listener for Agent operations
 */
export class OnceEventSubscription extends BaseSubscription {
  constructor(
    private readonly params: OnceAgentEventParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * Subscribe to events (auto-unsubscribes after first event)
   */
  subscribe(): () => void {
    return this.dependencies.getEventManager().once(this.params.eventType, this.params.listener);
  }

  /**
   * Get subscription metadata
   */
  getMetadata(): SubscriptionMetadata {
    return {
      name: "OnceAgentEvent",
      description: "Register a one-time event listener for Agent operations",
      eventType: this.params.eventType,
      requiresAuth: false,
      version: "1.0.0",
    };
  }
}
