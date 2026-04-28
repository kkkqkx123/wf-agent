/**
 * OnceEventSubscription - Registers a one-time event listener.
 */

import { BaseSubscription, SubscriptionMetadata } from "../../../../shared/types/subscription.js";
import type { EventType, EventListener, BaseEvent } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../../shared/core/sdk-dependencies.js";

/**
 * Registering one-time event listener parameters
 */
export interface OnceEventParams {
  /** Event Type */
  eventType: EventType;
  /** event listener */
  listener: EventListener<BaseEvent>;
}

/**
 * OnceEventSubscription - Registers a one-time event listener.
 */
export class OnceEventSubscription extends BaseSubscription {
  constructor(
    private readonly params: OnceEventParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * Subscribe to events
   */
  subscribe(): () => void {
    return this.dependencies.getEventManager().once(this.params.eventType, this.params.listener);
  }

  /**
   * Get subscription metadata
   */
  getMetadata(): SubscriptionMetadata {
    return {
      name: "OnceEvent",
      description: "Register a one-time event listener",
      eventType: this.params.eventType,
      requiresAuth: false,
      version: "1.0.0",
    };
  }
}
