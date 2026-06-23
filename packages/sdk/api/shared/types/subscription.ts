/**
 * Core Interface for Subscription Mode
 * Defines a unified interface for event subscription operations
 */

import type { EventType, EventListener, BaseEvent } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";

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
    private readonly options: {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
      executionId: string; // Required
    },
  ) {
    super();

    // Validate executionId is provided
    if (!options.executionId) {
      throw new RuntimeValidationError("executionId is required for event subscriptions", {
        field: "options.executionId",
      });
    }
  }

  subscribe(): () => void {
    // Use new EventEmitter API
    const emitter = this.eventManager.getEmitter(this.options.executionId);
    return emitter.on(this.eventType, this.listener, {
      filter: this.options.filter,
      timeout: this.options.timeout,
    });
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
    private readonly options: {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
      executionId: string; // Required
    },
  ) {
    super();

    // Validate executionId is provided
    if (!options.executionId) {
      throw new RuntimeValidationError("executionId is required for event subscriptions", {
        field: "options.executionId",
      });
    }
  }

  subscribe(): () => void {
    // Use new EventEmitter API
    const emitter = this.eventManager.getEmitter(this.options.executionId);
    return emitter.once(this.eventType, this.listener, {
      filter: this.options.filter,
      timeout: this.options.timeout,
    });
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
 * Factory function to create OnEventSubscription using APIDependencyManager
 * Useful for API layer operations that use dependency injection
 */
export function createOnEventSubscription<T extends BaseEvent = BaseEvent>(
  eventType: EventType,
  listener: EventListener<T>,
  dependencies: any, // APIDependencyManager type (avoid circular dependency)
  options: {
    priority?: number;
    filter?: (event: T) => boolean;
    timeout?: number;
    executionId: string;
  },
): OnEventSubscription<T> {
  const eventManager = dependencies.getEventManager();
  return new OnEventSubscription(eventType, listener, eventManager, options);
}

/**
 * Factory function to create OnceEventSubscription using APIDependencyManager
 * Useful for API layer operations that use dependency injection
 */
export function createOnceEventSubscription<T extends BaseEvent = BaseEvent>(
  eventType: EventType,
  listener: EventListener<T>,
  dependencies: any, // APIDependencyManager type (avoid circular dependency)
  options: {
    priority?: number;
    filter?: (event: T) => boolean;
    timeout?: number;
    executionId: string;
  },
): OnceEventSubscription<T> {
  const eventManager = dependencies.getEventManager();
  return new OnceEventSubscription(eventType, listener, eventManager, options);
}

/**
 * Helper to create execution-scoped subscription
 * Automatically injects executionId into options
 */
export function createExecutionScopedSubscription<T extends BaseEvent = BaseEvent>(
  executionId: string,
  eventType: EventType,
  listener: EventListener<T>,
  dependencies: any, // APIDependencyManager type
  additionalOptions?: Omit<
    {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
      executionId: string;
    },
    "executionId"
  >,
): OnEventSubscription<T> {
  return createOnEventSubscription(eventType, listener, dependencies, {
    ...additionalOptions,
    executionId,
  });
}

/**
 * Helper to create execution-scoped once subscription
 * Automatically injects executionId into options
 */
export function createExecutionScopedOnceSubscription<T extends BaseEvent = BaseEvent>(
  executionId: string,
  eventType: EventType,
  listener: EventListener<T>,
  dependencies: any, // APIDependencyManager type
  additionalOptions?: Omit<
    {
      priority?: number;
      filter?: (event: T) => boolean;
      timeout?: number;
      executionId: string;
    },
    "executionId"
  >,
): OnceEventSubscription<T> {
  return createOnceEventSubscription(eventType, listener, dependencies, {
    ...additionalOptions,
    executionId,
  });
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
    private readonly executionId: string, // Required
    private readonly timeout: number | undefined,
    private readonly eventManager: EventRegistry,
  ) {
    super();

    if (!executionId) {
      throw new RuntimeValidationError("executionId is required for event subscriptions", {
        field: "executionId",
      });
    }
  }

  subscribe(): () => void {
    // Use new EventEmitter API
    const emitter = this.eventManager.getEmitter(this.executionId);

    const listener = (event: BaseEvent) => {
      if (this.resolve) {
        this.resolve(event);
        this.cleanup();
      }
    };

    this.unsubscribe = emitter.on(this.eventType, listener);

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
