/**
 * Tool Description Registry
 *
 * Provides a registry for looking up ToolDescriptionData by tool ID.
 * This bridges the gap between Tool (runtime type) and ToolDescriptionData (prompt template type).
 *
 * Design Principles:
 * - Lazy initialization: Predefined tool descriptions are loaded on first use
 * - Extensible: Custom tool descriptions can be registered
 * - Type-safe: Full TypeScript type support
 */

import type { ToolDescriptionData } from "@wf-agent/prompt-templates";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ToolDescriptionRegistry" });

/**
 * Tool Description Registry Class
 */
export class ToolDescriptionRegistry {
  private static instance: ToolDescriptionRegistry | null = null;
  private descriptions = new Map<string, ToolDescriptionData>();
  private initialized = false;

  /**
   * Get registry instance (singleton)
   */
  static getInstance(): ToolDescriptionRegistry {
    if (!ToolDescriptionRegistry.instance) {
      ToolDescriptionRegistry.instance = new ToolDescriptionRegistry();
    }
    return ToolDescriptionRegistry.instance;
  }

  /**
   * Reset registry instance (mainly for testing)
   */
  static resetInstance(): void {
    ToolDescriptionRegistry.instance = null;
  }

  /**
   * Private constructor
   */
  private constructor() {}

  /**
   * Check if registry has been initialized
   */
  isInitialized(): boolean {
    return this.initialized;
  }

  /**
   * Register a tool description
   * @param description Tool description data
   */
  register(description: ToolDescriptionData): void {
    if (this.descriptions.has(description.id)) {
      logger.warn(`Tool description for '${description.id}' already exists, will be overwritten`);
    }
    this.descriptions.set(description.id, description);
  }

  /**
   * Batch register tool descriptions
   * @param descriptions Array of tool descriptions
   */
  registerAll(descriptions: ToolDescriptionData[]): void {
    for (const description of descriptions) {
      this.register(description);
    }
  }

  /**
   * Get tool description by ID
   * @param id Tool ID
   * @returns Tool description or undefined if not found
   */
  get(id: string): ToolDescriptionData | undefined {
    return this.descriptions.get(id);
  }

  /**
   * Check if tool description exists
   * @param id Tool ID
   */
  has(id: string): boolean {
    return this.descriptions.has(id);
  }

  /**
   * Get all registered tool descriptions
   */
  getAll(): ToolDescriptionData[] {
    return Array.from(this.descriptions.values());
  }

  /**
   * Get tool descriptions by category
   * @param category Tool category
   */
  getByCategory(category: ToolDescriptionData["category"]): ToolDescriptionData[] {
    return this.getAll().filter(d => d.category === category);
  }

  /**
   * Remove tool description
   * @param id Tool ID
   */
  unregister(id: string): boolean {
    return this.descriptions.delete(id);
  }

  /**
   * Clear all tool descriptions
   */
  clear(): void {
    this.descriptions.clear();
    this.initialized = false;
  }

  /**
   * Get number of registered descriptions
   */
  get size(): number {
    return this.descriptions.size;
  }

  /**
   * Get all tool IDs
   */
  getToolIds(): string[] {
    return Array.from(this.descriptions.keys());
  }
}

/**
 * Global registry instance
 */
export const toolDescriptionRegistry = ToolDescriptionRegistry.getInstance();

/**
 * Quick register function
 * @param description Tool description
 */
export function registerToolDescription(description: ToolDescriptionData): void {
  toolDescriptionRegistry.register(description);
}

/**
 * Quick batch register function
 * @param descriptions Array of tool descriptions
 */
export function registerToolDescriptions(descriptions: ToolDescriptionData[]): void {
  toolDescriptionRegistry.registerAll(descriptions);
}

/**
 * Quick lookup function
 * @param id Tool ID
 */
export function getToolDescription(id: string): ToolDescriptionData | undefined {
  return toolDescriptionRegistry.get(id);
}

/**
 * Check if tool description exists
 * @param id Tool ID
 */
export function hasToolDescription(id: string): boolean {
  return toolDescriptionRegistry.has(id);
}
