/**
 * OnEventSubscription - Registering an event listener
 */

import { BaseSubscription, SubscriptionMetadata } from "../../../../shared/types/subscription.js";
import type { EventType, EventListener, BaseEvent } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../../shared/core/sdk-dependencies.js";

/**
 * Register event listener parameters
 */
export interface OnEventParams {
  /** Event Type */
  eventType: EventType;
  /** Event listener */
  listener: EventListener<BaseEvent>;
}

/**
 * OnEventSubscription - Registers an event listener
 */
export class OnEventSubscription extends BaseSubscription {
  constructor(
    private readonly params: OnEventParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * Subscribe to events
   */
  subscribe(): () => void {
    return this.dependencies.getEventManager().on(this.params.eventType, this.params.listener);
  }

  /**
   * Get subscription metadata
   */
  getMetadata(): SubscriptionMetadata {
    return {
      name: "OnEvent",
      description: "Registering event listeners",
      eventType: this.params.eventType,
      requiresAuth: false,
      version: "1.0.0.0",
    };
  }
}
