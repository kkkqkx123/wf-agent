/**
 * OffEventSubscription - cancel the event listener
 */

import { BaseSubscription, SubscriptionMetadata } from "../../../shared/types/subscription.js";
import type { EventType, EventListener, BaseEvent } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Cancel event listener parameters
 */
export interface OffEventParams {
  /** Event Type */
  eventType: EventType;
  /** event listener */
  listener: EventListener<BaseEvent>;
}

/**
 * OffEventSubscription - cancel the event listener
 */
export class OffEventSubscription extends BaseSubscription {
  constructor(
    private readonly params: OffEventParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * unsubscribe
   */
  subscribe(): () => void {
    this.dependencies.getEventManager().off(this.params.eventType, this.params.listener);
    return () => {}; // has been canceled, returning the empty function
  }

  /**
   * Get subscription metadata
   */
  getMetadata(): SubscriptionMetadata {
    return {
      name: "OffEvent",
      description: "Unsubscribe from the event listener",
      eventType: this.params.eventType,
      requiresAuth: false,
      version: "1.0.0",
    };
  }
}
