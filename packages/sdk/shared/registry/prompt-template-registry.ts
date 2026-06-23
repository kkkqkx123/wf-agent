/**
 * PromptTemplateRegistry - Prompt Template Registry
 *
 * Provides unified template management and rendering capabilities.
 * Supports template registration, retrieval, and rendering.
 *
 * This module only exports class definitions; instances are managed by the DI container as singletons.
 */

import type { PromptTemplate } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { FragmentRegistry } from "./fragment-registry.js";
import { createRegistry } from "./utils/index.js";
import type { MutableRegistry } from "./types.js";
import { renderTemplate } from "../utils/template-renderer/index.js";
import { validatePromptTemplate } from "./utils/index.js";

const logger = createContextualLogger({ component: "PromptTemplateRegistry" });

/**
 * Prompt Template Registry Class
 *
 * Manages prompt templates with:
 * - Cross-registry validation with FragmentRegistry
 * - Template rendering with variable substitution
 * - Category-based querying
 */
export class PromptTemplateRegistry {
  private items: MutableRegistry<PromptTemplate>;
  private fragmentRegistry: FragmentRegistry | null = null;
  private initialized = false;

  constructor() {
    this.items = createRegistry<PromptTemplate>();
  }

  /**
   * Set the fragment registry for cross-registry reference validation.
   *
   * When set, registering a template with a `fragments` field will validate
   * that all referenced fragment IDs exist in the fragment registry.
   *
   * @param registry The FragmentRegistry instance
   */
  setFragmentRegistry(registry: FragmentRegistry): void {
    this.fragmentRegistry = registry;
  }

  /**
   * Check if it has been initialized.
   */
  isInitialized(): boolean {
    return this.initialized;
  }

  /**
   * Mark as initialized.
   */
  markInitialized(): void {
    this.initialized = true;
  }

  /**
   * Register a prompt template with cross-registry validation.
   *
   * @param key Unique template ID
   * @param template The template definition
   * @param options Registration options
   * @throws Error if template already exists and skipIfExists is not set
   */
  register(key: string, template: PromptTemplate, options?: { skipIfExists?: boolean }): void {
    if (this.items.has(key)) {
      if (options?.skipIfExists) {
        return;
      }
      throw new Error(`Item '${key}' already exists`);
    }

    validatePromptTemplate(template);

    // Validate cross-registry references: if template references fragments,
    // check they exist in the fragment registry (if one is configured).
    if (template.fragments && template.fragments.length > 0 && this.fragmentRegistry) {
      for (const fragmentId of template.fragments) {
        if (!this.fragmentRegistry.has(fragmentId)) {
          logger.warn(
            `Template '${key}' references fragment '${fragmentId}' ` +
              `which is not registered in FragmentRegistry`,
          );
        } else {
          // Record the dependency so fragment deletion can notify affected templates
          this.fragmentRegistry.addDependent(fragmentId, key);
        }
      }
    }

    this.items.set(key, template);
  }

  /**
   * Batch register multiple items.
   * @param items Array of items to register
   * @param options Registration options
   */
  registerAll(items: PromptTemplate[], options?: { skipIfExists?: boolean }): void {
    for (const item of items) {
      this.register(item.id, item, options);
    }
  }

  /**
   * Get an item by ID.
   * @param key Item ID
   * @returns The item or undefined if not found
   */
  get(key: string): PromptTemplate | undefined {
    return this.items.get(key);
  }

  /**
   * Check if an item exists.
   * @param key Item ID
   * @returns Whether the item exists
   */
  has(key: string): boolean {
    return this.items.has(key);
  }

  /**
   * Get all items.
   * @returns Array of all items
   */
  list(): PromptTemplate[] {
    return this.items.list();
  }

  /**
   * Get all item IDs.
   * @returns Array of all item IDs
   */
  keys(): string[] {
    return this.items.keys();
  }

  /**
   * Get the number of registered items.
   */
  get size(): number {
    return this.items.size;
  }

  /**
   * Unregister an item by ID.
   * @param key Item ID to remove
   * @returns Whether the item was removed
   */
  unregister(key: string): boolean {
    return this.items.delete(key);
  }

  /**
   * Clear all items.
   */
  clear(): void {
    this.items.clear();
    this.initialized = false;
    this.fragmentRegistry = null;
  }

  /**
   * Get templates of the specified category
   * @param category Template category
   */
  getByCategory(category: string): PromptTemplate[] {
    return this.list().filter(t => t.category === category);
  }

  /**
   * Render item content with variable substitution.
   * @param id Item ID
   * @param variables Variable values to substitute (optional)
   * @returns Rendered content string, or null if item not found
   */
  render(id: string, variables?: Record<string, unknown>): string | null {
    const item = this.get(id);
    if (!item) return null;

    if (!variables || Object.keys(variables).length === 0) {
      return item.content;
    }

    return renderTemplate(item.content, variables);
  }

  /**
   * Securely render a template (with default values)
   * @param id The template ID
   * @param variables The template variables
   * @param defaultValue The default value when the template does not exist
   * @returns The rendered string or the default value
   */
  renderSafe(id: string, variables: Record<string, unknown>, defaultValue: string = ""): string {
    const result = this.render(id, variables);
    return result ?? defaultValue;
  }

  /**
   * Get all registered template IDs.
   *
   * @returns Array of all template IDs
   */
  getTemplateIds(): string[] {
    return this.keys();
  }
}
