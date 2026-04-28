/**
 * Core Interface for Subscription Mode
 * Defines a unified interface for event subscription operations
 */

import type { EventType, EventListener, BaseEvent } from "@wf-agent/types";
import type { EventRegistry } from "../../../core/registry/event-registry.js";

/**
 * Subscription Metadata
 */
export interface SubscriptionMetadata {
  /** Subscription Name */
  name: string;
  /** Subscription Description */
  description: string;
  /** Event Type */
  eventType: EventType;
  /** Is authentication required? */
  requiresAuth: boolean;
  /** Version */
  version: string;
}

/**
 * Subscription Interface
 * All subscription operations must implement this interface.
 */
export interface Subscription<_T extends BaseEvent = BaseEvent> {
  /**
   * Subscribe to an event
   * @returns Function to unsubscribe
   */
  subscribe(): () => void;

  /**
   * Retrieve subscription metadata
   * @returns Subscription metadata
   */
  getMetadata(): SubscriptionMetadata;
}

/**
 * Abstract Subscription Base Class
 * Provides a general implementation for subscriptions
 */
export abstract class BaseSubscription<T extends BaseEvent = BaseEvent> implements Subscription<T> {
  /**
   * Subscribe to events
   */
  abstract subscribe(): () => void;

  /**
   * Retrieve subscription metadata
   */
  abstract getMetadata(): SubscriptionMetadata;
}

/**
 * OnEventSubscription - Registers an event listener
 */
export class OnEventSubscription<T extends BaseEvent = BaseEvent> extends BaseSubscription<T> {
  constructor(
    private readonly eventType: EventType,
    private readonly listener: EventListener<T>,
    private readonly eventManager: EventRegistry,
  ) {
    super();
  }

  subscribe(): () => void {
    return this.eventManager.on(this.eventType, this.listener);
  }

  getMetadata(): SubscriptionMetadata {
    return {
      name: "OnEvent",
      description: "Register an event listener",
      eventType: this.eventType,
      requiresAuth: false,
      version: "1.0.0",
    };
  }
}

/**
 * OnceEventSubscription - Registers a one-time event listener
 */
export class OnceEventSubscription<T extends BaseEvent = BaseEvent> extends BaseSubscription<T> {
  constructor(
    private readonly eventType: EventType,
    private readonly listener: EventListener<T>,
    private readonly eventManager: EventRegistry,
  ) {
    super();
  }

  subscribe(): () => void {
    return this.eventManager.once(this.eventType, this.listener);
  }

  getMetadata(): SubscriptionMetadata {
    return {
      name: "OnceEvent",
      description: "Register a one-time event listener",
      eventType: this.eventType,
      requiresAuth: false,
      version: "1.0.0",
    };
  }
}

/**
 * WaitForEventSubscription - Wait for a specific event to be triggered.
 */
export class WaitForEventSubscription extends BaseSubscription {
  private unsubscribe: (() => void) | null = null;
  private resolve: ((event: BaseEvent) => void) | null = null;
  private reject: ((error: Error) => void) | null = null;
  private timeoutId: NodeJS.Timeout | null = null;

  constructor(
    private readonly eventType: EventType,
    private readonly timeout: number | undefined,
    private readonly eventManager: EventRegistry,
  ) {
    super();
  }

  subscribe(): () => void {
    const listener = (event: BaseEvent) => {
      if (this.resolve) {
        this.resolve(event);
        this.cleanup();
      }
    };

    this.unsubscribe = this.eventManager.on(this.eventType, listener);

    if (this.timeout !== undefined && this.timeout > 0) {
      this.timeoutId = setTimeout(() => {
        if (this.reject) {
          this.reject(new Error(`Timeout waiting for event: ${this.eventType}`));
          this.cleanup();
        }
      }, this.timeout);
    }

    return () => this.cleanup();
  }

  /**
   * Waiting for an event
   */
  async wait(): Promise<BaseEvent> {
    return new Promise((resolve, reject) => {
      this.resolve = resolve;
      this.reject = reject;
      this.subscribe();
    });
  }

  private cleanup(): void {
    if (this.unsubscribe) {
      this.unsubscribe();
      this.unsubscribe = null;
    }
    if (this.timeoutId) {
      clearTimeout(this.timeoutId);
      this.timeoutId = null;
    }
    this.resolve = null;
    this.reject = null;
  }

  getMetadata(): SubscriptionMetadata {
    return {
      name: "WaitForEvent",
      description: "Wait for a specific event to trigger.",
      eventType: this.eventType,
      requiresAuth: false,
      version: "1.0.0",
    };
  }
}
