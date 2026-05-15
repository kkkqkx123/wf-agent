/**
 * OnEventSubscription - Registering an event listener
 * 
 * All event listeners must be execution-scoped:
 * - executionId is REQUIRED
 * - Automatically cleaned up when execution ends
 */

import { BaseSubscription, SubscriptionMetadata } from "../../../shared/types/subscription.js";
import type { EventType, EventListener, BaseEvent } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Register event listener parameters
 */
export interface OnEventParams {
  /** Event Type */
  eventType: EventType;
  /** Event listener */
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
 * Helper function to create execution-scoped subscription
 * Automatically injects executionId into options
 */
export function createExecutionScopedSubscription(
  executionId: string,
  eventType: EventType,
  listener: EventListener<BaseEvent>,
  dependencies: APIDependencyManager,
  additionalOptions?: Omit<OnEventParams['options'], 'executionId'>,
): OnEventSubscription {
  return new OnEventSubscription(
    {
      eventType,
      listener,
      options: {
        ...additionalOptions,
        executionId,
      },
    },
    dependencies,
  );
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
    const emitter = this.dependencies.getEventManager().getEmitter(this.params.options.executionId);
    return emitter.on(this.params.eventType, this.params.listener, {
      filter: this.params.options.filter,
      timeout: this.params.options.timeout,
    });
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
      version: "1.0.0",
    };
  }
}
