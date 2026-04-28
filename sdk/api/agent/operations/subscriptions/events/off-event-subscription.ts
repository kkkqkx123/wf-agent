/**
 * OffEventSubscription - Cancel Agent Event Listener
 *
 * Responsibilities:
 * - Encapsulates event listener cancellation as Subscription pattern
 * - Provides unified API layer interface for Agent events
 * - Supports all event types from EventRegistry
 *
 * Design Principles:
 * - Follows Subscription pattern, inherits BaseSubscription
 * - Uses dependency injection for EventRegistry
 * - Returns empty function (already cancelled)
 */

import { BaseSubscription, SubscriptionMetadata } from "../../../../shared/types/subscription.js";
import type { EventType, EventListener, BaseEvent } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../../shared/core/sdk-dependencies.js";

/**
 * Cancel event listener parameters
 */
export interface OffAgentEventParams {
  /** Event Type */
  eventType: EventType;
  /** Event listener */
  listener: EventListener<BaseEvent>;
}

/**
 * OffEventSubscription - Cancel an event listener for Agent operations
 */
export class OffEventSubscription extends BaseSubscription {
  constructor(
    private readonly params: OffAgentEventParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * Unsubscribe from events
   */
  subscribe(): () => void {
    this.dependencies.getEventManager().off(this.params.eventType, this.params.listener);
    return () => {};
  }

  /**
   * Get subscription metadata
   */
  getMetadata(): SubscriptionMetadata {
    return {
      name: "OffAgentEvent",
      description: "Cancel an event listener for Agent operations",
      eventType: this.params.eventType,
      requiresAuth: false,
      version: "1.0.0",
    };
  }
}
