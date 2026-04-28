/**
 * Message Bus Implementation
 *
 * Core message distribution system for component messages.
 */

import type {
  BaseComponentMessage,
  CreateMessageInput,
  MessageCategory,
  OutputHandler,
  OutputTarget,
  RoutingRule,
} from "@wf-agent/types";
import { matchesRoutingRule } from "./routing-utils.js";

/**
 * Message Filter
 */
export interface MessageFilter {
  /** Filter by categories */
  categories?: MessageCategory[];
  /** Filter by types */
  types?: string[];
  /** Filter by entity IDs */
  entityIds?: string[];
  /** Filter by levels */
  levels?: string[];
  /** Filter by timestamp (messages after this time) */
  since?: number;
  /** Filter by timestamp (messages before this time) */
  until?: number;
}

/**
 * Message Handler (subscriber)
 */
export type MessageHandler = (message: BaseComponentMessage) => void | Promise<void>;

/**
 * Message Subscription
 */
export interface MessageSubscription {
  /** Unsubscribe from messages */
  unsubscribe(): void;
  /** Check if subscription is active */
  readonly active: boolean;
}

/**
 * Subscriber Entry
 */
interface SubscriberEntry {
  filter: MessageFilter;
  handler: MessageHandler;
  active: boolean;
}

/**
 * Message Bus Options
 */
export interface MessageBusOptions {
  /** Maximum history size (default: 1000) */
  maxHistorySize?: number;
  /** Enable history (default: true) */
  enableHistory?: boolean;
  /** Enable async handler execution (default: true) */
  asyncHandlers?: boolean;
}

/**
 * Entity Status
 */
export type EntityStatus = "running" | "paused" | "completed" | "error" | "cancelled";

/**
 * Entity Context
 */
export interface EntityContext {
  type: string;
  id: string;
  parentId?: string;
  rootId: string;
  depth: number;
  parallelGroup?: {
    groupId: string;
    branchIndex: number;
    totalBranches: number;
  };
  createdAt: number;
  updatedAt: number;
  status: EntityStatus;
  metadata?: Record<string, unknown>;
}

/**
 * Message Bus
 *
 * Central hub for publishing and subscribing to component messages.
 */
export class MessageBus {
  private handlers: Map<string, OutputHandler> = new Map();
  private subscribers: SubscriberEntry[] = [];
  private history: BaseComponentMessage[] = [];
  private rules: RoutingRule[] = [];
  private entities: Map<string, EntityContext> = new Map();
  private sequence: number = 0;
  private options: Required<MessageBusOptions>;

  /**
   * Create a new MessageBus
   * @param rules Initial routing rules
   * @param options Bus options
   */
  constructor(rules: RoutingRule[] = [], options: MessageBusOptions = {}) {
    this.rules = [...rules].sort((a, b) => a.priority - b.priority);
    this.options = {
      maxHistorySize: options.maxHistorySize ?? 1000,
      enableHistory: options.enableHistory ?? true,
      asyncHandlers: options.asyncHandlers ?? true,
    };
  }

  /**
   * Publish a message
   * @param input Message input (without auto-generated fields)
   */
  publish(input: CreateMessageInput): void {
    const message: BaseComponentMessage = {
      ...input,
      id: this.generateId(input),
      timestamp: Date.now(),
    };

    // Add to history
    if (this.options.enableHistory) {
      this.history.push(message);
      if (this.history.length > this.options.maxHistorySize) {
        this.history.shift();
      }
    }

    // Decide output targets
    const decision = this.decideOutput(message);

    // Route to handlers
    for (const target of decision.targets) {
      const handler = this.findHandler(target, message);
      if (handler) {
        if (this.options.asyncHandlers) {
          const result = handler.handle(message);
          if (result instanceof Promise) {
            result.catch((err: unknown) => {
              console.error(`Handler ${handler.name} failed:`, err);
            });
          }
        } else {
          handler.handle(message);
        }
      }
    }

    // Notify subscribers
    this.notifySubscribers(message);
  }

  /**
   * Subscribe to messages
   * @param filter Message filter
   * @param handler Message handler
   * @returns Subscription handle
   */
  subscribe(filter: MessageFilter, handler: MessageHandler): MessageSubscription {
    const entry: SubscriberEntry = {
      filter,
      handler,
      active: true,
    };

    this.subscribers.push(entry);

    return {
      unsubscribe: () => {
        entry.active = false;
        const index = this.subscribers.indexOf(entry);
        if (index >= 0) {
          this.subscribers.splice(index, 1);
        }
      },
      get active() {
        return entry.active;
      },
    };
  }

  /**
   * Subscribe to all messages
   * @param handler Message handler
   * @returns Subscription handle
   */
  subscribeAll(handler: MessageHandler): MessageSubscription {
    return this.subscribe({}, handler);
  }

  /**
   * Configure routing rules
   * @param rules Routing rules
   */
  configureRouting(rules: RoutingRule[]): void {
    this.rules = [...rules].sort((a, b) => a.priority - b.priority);
  }

  /**
   * Add a routing rule
   * @param rule Routing rule
   */
  addRoutingRule(rule: RoutingRule): void {
    this.rules.push(rule);
    this.rules.sort((a, b) => a.priority - b.priority);
  }

  /**
   * Register an output handler
   * @param handler Output handler
   */
  registerHandler(handler: OutputHandler): void {
    this.handlers.set(handler.name, handler);
  }

  /**
   * Unregister an output handler
   * @param name Handler name
   * @returns true if handler was found and removed
   */
  unregisterHandler(name: string): boolean {
    return this.handlers.delete(name);
  }

  /**
   * Get message history
   * @param filter Optional filter
   * @returns Array of messages
   */
  getHistory(filter?: MessageFilter): BaseComponentMessage[] {
    if (!this.options.enableHistory) {
      return [];
    }

    if (!filter) {
      return [...this.history];
    }

    return this.history.filter(msg => this.matchesFilter(msg, filter));
  }

  /**
   * Clear message history
   */
  clearHistory(): void {
    this.history = [];
  }

  /**
   * Get history size
   * @returns Number of messages in history
   */
  getHistorySize(): number {
    return this.history.length;
  }

  /**
   * Flush all handlers
   */
  async flush(): Promise<void> {
    const flushPromises: Promise<void>[] = [];

    for (const handler of this.handlers.values()) {
      if (handler.flush) {
        flushPromises.push(handler.flush());
      }
    }

    await Promise.all(flushPromises);
  }

  /**
   * Close all handlers
   */
  async close(): Promise<void> {
    const closePromises: Promise<void>[] = [];

    for (const handler of this.handlers.values()) {
      if (handler.close) {
        closePromises.push(handler.close());
      }
    }

    await Promise.all(closePromises);
    this.handlers.clear();
    this.subscribers = [];
  }

  /**
   * Register an entity
   */
  registerEntity(
    identity: { type: string; id: string; parentId?: string; rootId: string; depth: number },
    metadata?: Record<string, unknown>,
  ): EntityContext {
    const now = Date.now();
    const context: EntityContext = {
      ...identity,
      createdAt: now,
      updatedAt: now,
      status: "running",
      metadata,
    };

    this.entities.set(context.id, context);
    return context;
  }

  /**
   * Get an entity by ID
   */
  getEntity(entityId: string): EntityContext | undefined {
    return this.entities.get(entityId);
  }

  /**
   * Update entity status
   */
  updateEntityStatus(entityId: string, status: EntityStatus): boolean {
    const entity = this.entities.get(entityId);
    if (entity) {
      entity.status = status;
      entity.updatedAt = Date.now();
      return true;
    }
    return false;
  }

  /**
   * Generate a unique message ID
   */
  private generateId(input: CreateMessageInput): string {
    return `${input.category}:${input.entity.type}:${input.entity.id}:${Date.now()}:${++this.sequence}`;
  }

  /**
   * Decide output for a message
   */
  private decideOutput(message: BaseComponentMessage): { targets: OutputTarget[] } {
    for (const rule of this.rules) {
      if (matchesRoutingRule(message, rule)) {
        return { targets: rule.decision.targets };
      }
    }

    // Default: no output
    return { targets: [] };
  }

  /**
   * Find a handler for a target and message
   */
  private findHandler(
    target: OutputTarget,
    message: BaseComponentMessage,
  ): OutputHandler | undefined {
    for (const handler of this.handlers.values()) {
      if (handler.target === target && handler.supports(message)) {
        return handler;
      }
    }
    return undefined;
  }

  /**
   * Notify all matching subscribers
   */
  private notifySubscribers(message: BaseComponentMessage): void {
    for (const entry of this.subscribers) {
      if (entry.active && this.matchesFilter(message, entry.filter)) {
        try {
          const result = entry.handler(message);
          if (result instanceof Promise) {
            result.catch(err => {
              console.error("Subscriber handler failed:", err);
            });
          }
        } catch (err) {
          console.error("Subscriber handler failed:", err);
        }
      }
    }
  }

  /**
   * Check if a message matches a filter
   */
  private matchesFilter(message: BaseComponentMessage, filter: MessageFilter): boolean {
    if (filter.categories && !filter.categories.includes(message.category)) {
      return false;
    }

    if (filter.types && !filter.types.includes(message.type)) {
      return false;
    }

    if (filter.entityIds && !filter.entityIds.includes(message.entity.id)) {
      return false;
    }

    if (filter.levels && !filter.levels.includes(message.level)) {
      return false;
    }

    if (filter.since && message.timestamp < filter.since) {
      return false;
    }

    if (filter.until && message.timestamp > filter.until) {
      return false;
    }

    return true;
  }
}

/**
 * Create a message bus with default configuration
 * @param rules Optional routing rules
 * @returns Message bus instance
 */
export function createMessageBus(rules?: RoutingRule[]): MessageBus {
  return new MessageBus(rules);
}
