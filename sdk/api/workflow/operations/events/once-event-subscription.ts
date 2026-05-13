/**
 * OnceEventSubscription - Registers a one-time event listener.
 * 
 * All event listeners must be execution-scoped:
 * - executionId is REQUIRED
 * - Automatically cleaned up when execution ends
 */

import { BaseSubscription, SubscriptionMetadata } from "../../../shared/types/subscription.js";
import type { EventType, EventListener, BaseEvent } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Registering one-time event listener parameters
 */
export interface OnceEventParams {
  /** Event Type */
  eventType: EventType;
  /** event listener */
  listener: EventListener<BaseEvent>;
  /** Listener options - executionId is required */
  options: {
    priority?: number;
    filter?: (event: BaseEvent) => boolean;
    timeout?: number;
    executionId: string; // Required execution ID
  };
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
    const emitter = this.dependencies.getEventManager().getEmitter(this.params.options.executionId);
    return emitter.once(this.params.eventType, this.params.listener, {
      filter: this.params.options.filter,
      timeout: this.params.options.timeout,
    });
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
