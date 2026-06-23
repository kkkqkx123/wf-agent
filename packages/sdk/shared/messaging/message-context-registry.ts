/**
 * Message Context Registry Implementation
 *
 * In-memory implementation of the MessageContextRegistry interface.
 * Manages named message contexts within a workflow execution.
 */

import type { NamedMessageContext, MessageContextRegistry } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger();

/**
 * In-memory Message Context Registry
 *
 * Provides CRUD operations for managing named message contexts.
 * Thread-safe for single-threaded execution environments.
 */
export class InMemoryMessageContextRegistry implements MessageContextRegistry {
  private contexts: Map<string, NamedMessageContext> = new Map();

  /**
   * Register a new context
   * @param context The context to register
   * @throws Error if context with same ID already exists
   */
  register(context: NamedMessageContext): void {
    if (this.contexts.has(context.id)) {
      logger.warn(`Context '${context.id}' already exists, overwriting`, {
        contextId: context.id,
      });
    }

    this.contexts.set(context.id, {
      ...context,
      createdAt: context.createdAt || now(),
      updatedAt: context.updatedAt || now(),
    });

    logger.debug(`Registered context '${context.id}'`, {
      contextId: context.id,
      messageCount: context.messages.length,
      description: context.metadata?.description,
    });
  }

  /**
   * Get a context by ID
   * @param id Context ID
   * @returns The context or undefined if not found
   */
  get(id: string): NamedMessageContext | undefined {
    return this.contexts.get(id);
  }

  /**
   * Update messages in an existing context
   * @param id Context ID
   * @param messages New message array
   * @throws Error if context does not exist
   */
  update(id: string, messages: NamedMessageContext["messages"]): void {
    const context = this.contexts.get(id);

    if (!context) {
      throw new Error(
        `Context '${id}' not found. Available contexts: ${this.listIds().join(", ")}`,
      );
    }

    context.messages = messages;
    context.updatedAt = now();

    logger.debug(`Updated context '${id}'`, {
      contextId: id,
      messageCount: messages.length,
    });
  }

  /**
   * Delete a context by ID
   * @param id Context ID
   * @returns true if context was deleted, false if not found
   */
  delete(id: string): boolean {
    const existed = this.contexts.delete(id);

    if (existed) {
      logger.debug(`Deleted context '${id}'`, { contextId: id });
    } else {
      logger.warn(`Attempted to delete non-existent context '${id}'`, { contextId: id });
    }

    return existed;
  }

  /**
   * List all registered context IDs
   * @returns Array of context IDs
   */
  listIds(): string[] {
    return Array.from(this.contexts.keys());
  }

  /**
   * Check if a context exists
   * @param id Context ID
   * @returns true if context exists
   */
  has(id: string): boolean {
    return this.contexts.has(id);
  }

  /**
   * Get the total number of registered contexts
   * @returns Context count
   */
  size(): number {
    return this.contexts.size;
  }

  /**
   * Clear all contexts
   */
  clear(): void {
    const count = this.contexts.size;
    this.contexts.clear();
    logger.debug(`Cleared all contexts`, { clearedCount: count });
  }

  /**
   * Get a snapshot of all contexts (for debugging/serialization)
   * @returns Array of all contexts
   */
  getAll(): NamedMessageContext[] {
    return Array.from(this.contexts.values());
  }
}
