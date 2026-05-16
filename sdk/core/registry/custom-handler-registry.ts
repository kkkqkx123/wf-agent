/**
 * Custom Trigger Handler Registry
 *
 * Provides a registry for custom trigger handlers that can be registered at runtime.
 * This allows users to define their own custom actions for triggers.
 *
 * Design Principles:
 * - Managed by DI container as singleton: Single registry instance across the application
 * - Type-safe: Strongly typed handler functions
 * - Error handling: Clear errors for missing handlers
 */

import type { BaseTriggerDefinition, TriggerExecutionResult } from "../triggers/types.js";
import { RuntimeValidationError } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "CustomHandlerRegistry" });

/**
 * Custom handler function type
 */
export type CustomTriggerHandler = (
  trigger: BaseTriggerDefinition,
  parameters: Record<string, unknown>,
) => Promise<TriggerExecutionResult>;

/**
 * Custom Handler Registry
 *
 * Manages the registration and lookup of custom trigger handlers.
 */
export class CustomHandlerRegistry {
  private handlers: Map<string, CustomTriggerHandler> = new Map();

  /**
   * Register a custom handler
   *
   * @param name Unique handler name
   * @param handler Handler function
   * @throws RuntimeValidationError if handler name is already registered
   */
  register(name: string, handler: CustomTriggerHandler): void {
    if (this.handlers.has(name)) {
      throw new RuntimeValidationError(`Custom handler '${name}' is already registered`, {
        operation: "register",
        field: "name",
        value: name,
      });
    }

    this.handlers.set(name, handler);
    logger.info("Custom handler registered", { handlerName: name });
  }

  /**
   * Unregister a custom handler
   *
   * @param name Handler name
   * @returns true if handler was found and removed, false otherwise
   */
  unregister(name: string): boolean {
    const existed = this.handlers.delete(name);
    if (existed) {
      logger.info("Custom handler unregistered", { handlerName: name });
    }
    return existed;
  }

  /**
   * Get a registered handler
   *
   * @param name Handler name
   * @returns The handler function, or undefined if not found
   */
  getHandler(name: string): CustomTriggerHandler | undefined {
    return this.handlers.get(name);
  }

  /**
   * Check if a handler is registered
   *
   * @param name Handler name
   * @returns true if handler exists
   */
  hasHandler(name: string): boolean {
    return this.handlers.has(name);
  }

  /**
   * Get all registered handler names
   *
   * @returns Array of handler names
   */
  getRegisteredNames(): string[] {
    return Array.from(this.handlers.keys());
  }

  /**
   * Clear all registered handlers
   */
  clear(): void {
    this.handlers.clear();
    logger.info("All custom handlers cleared");
  }

  /**
   * Get the number of registered handlers
   */
  get size(): number {
    return this.handlers.size;
  }
}
