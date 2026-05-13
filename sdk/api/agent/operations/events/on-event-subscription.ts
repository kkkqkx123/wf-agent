/**
 * OnEventSubscription - Register Agent Event Listener
 *
 * Responsibilities:
 * - Encapsulates event listener registration as Subscription pattern
 * - Provides unified API layer interface for Agent events
 * - Supports all event types from EventRegistry
 * 
 * All event listeners must be execution-scoped:
 * - executionId is REQUIRED
 * - Automatically cleaned up when execution ends
 *
 * Design Principles:
 * - Follows Subscription pattern, inherits BaseSubscription
 * - Uses dependency injection for EventRegistry
 * - Returns unsubscribe function
 */

import { BaseSubscription, SubscriptionMetadata } from "../../../shared/types/subscription.js";
import type { EventType, EventListener, BaseEvent } from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Register event listener parameters
 */
export interface OnAgentEventParams {
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
 * Helper function to create execution-scoped subscription for Agent
 * Automatically injects executionId into options
 */
export function createExecutionScopedAgentSubscription(
  executionId: string,
  eventType: EventType,
  listener: EventListener<BaseEvent>,
  dependencies: APIDependencyManager,
  additionalOptions?: Omit<OnAgentEventParams['options'], 'executionId'>,
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
 * OnEventSubscription - Registers an event listener for Agent operations
 */
export class OnEventSubscription extends BaseSubscription {
  constructor(
    private readonly params: OnAgentEventParams,
    private readonly dependencies: APIDependencyManager,
  ) {
    super();
  }

  /**
   * Subscribe to events
   */
  subscribe(): () => void {
    return this.dependencies.getEventManager().on(this.params.eventType, this.params.listener, this.params.options);
  }

  /**
   * Get subscription metadata
   */
  getMetadata(): SubscriptionMetadata {
    return {
      name: "OnAgentEvent",
      description: "Register an event listener for Agent operations",
      eventType: this.params.eventType,
      requiresAuth: false,
      version: "1.0.0",
    };
  }
}
