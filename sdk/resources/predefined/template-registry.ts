/**
 * PromptTemplateRegistry - Prompt Template Registry
 *
 * Provides unified template management and rendering capabilities
 * Supports template registration, retrieval, and rendering
 *
 * Design Principles:
 * - Singleton pattern: A globally unique instance of the registry
 * - Lazy initialization: Predefined templates are loaded only the first time they are used
 * - Type safety: Full TypeScript type support
 */

import type { PromptTemplate } from "@wf-agent/prompt-templates";
import { renderTemplate } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "PromptTemplateRegistry" });

/**
 * Template Registry Class
 */
export class PromptTemplateRegistry {
  private static instance: PromptTemplateRegistry | null = null;
  private templates = new Map<string, PromptTemplate>();
  private initialized = false;

  /**
   * Obtain a registry instance (singleton).
   */
  static getInstance(): PromptTemplateRegistry {
    if (!PromptTemplateRegistry.instance) {
      PromptTemplateRegistry.instance = new PromptTemplateRegistry();
    }
    return PromptTemplateRegistry.instance;
  }

  /**
   * Resetting the registry instance (mainly for testing purposes)
   */
  static resetInstance(): void {
    PromptTemplateRegistry.instance = null;
  }

  /**
   * Private constructor
   */
  private constructor() {}

  /**
   * Check if it has been initialized.
   */
  isInitialized(): boolean {
    return this.initialized;
  }

  /**
   * Register a single template
   * @param template Template definition
   */
  register(template: PromptTemplate): void {
    if (this.templates.has(template.id)) {
      logger.warn(`Template with id '${template.id}' already exists, will be overwritten`);
    }
    this.templates.set(template.id, template);
  }

  /**
   * Batch registration template
   * @param templates Array of templates
   */
  registerAll(templates: PromptTemplate[]): void {
    for (const template of templates) {
      this.register(template);
    }
  }

  /**
   * Get the template
   * @param id: Template ID
   * @returns: Template definition; returns undefined if not found
   */
  get(id: string): PromptTemplate | undefined {
    return this.templates.get(id);
  }

  /**
   * Check if the template exists
   * @param id Template ID
   */
  has(id: string): boolean {
    return this.templates.has(id);
  }

  /**
   * Get all registered templates
   */
  getAll(): PromptTemplate[] {
    return Array.from(this.templates.values());
  }

  /**
   * Get templates of the specified category
   * @param category Template category
   */
  getByCategory(category: string): PromptTemplate[] {
    return this.getAll().filter(t => t.category === category);
  }

  /**
   * Remove the template
   * @param id Template ID
   */
  unregister(id: string): boolean {
    return this.templates.delete(id);
  }

  /**
   * Clear all templates.
   */
  clear(): void {
    this.templates.clear();
    this.initialized = false;
  }

  /**
   * Render template
   * @param id: Template ID
   * @param variables: Template variables
   * @returns: The rendered string; returns null if the template does not exist
   */
  render(id: string, variables: Record<string, unknown>): string | null {
    const template = this.get(id);
    if (!template) {
      return null;
    }
    return renderTemplate(template.content, variables);
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
   * Get all template IDs
   */
  getTemplateIds(): string[] {
    return Array.from(this.templates.keys());
  }

  /**
   * Get the number of templates
   */
  get size(): number {
    return this.templates.size;
  }
}

/**
 * Global Registry Instance
 */
export const templateRegistry = PromptTemplateRegistry.getInstance();

/**
 * Quick registration template
 * @param template Template definition
 */
export function registerTemplate(template: PromptTemplate): void {
  templateRegistry.register(template);
}

/**
 * Quick batch registration template
 * @param templates Array of templates
 */
export function registerTemplates(templates: PromptTemplate[]): void {
  templateRegistry.registerAll(templates);
}

/**
 * Quickly retrieve a template
 * @param id Template ID
 */
export function getTemplate(id: string): PromptTemplate | undefined {
  return templateRegistry.get(id);
}

/**
 * Quick template rendering
 * @param id Template ID
 * @param variables Template variables
 */
export function renderTemplateById(id: string, variables: Record<string, unknown>): string | null {
  return templateRegistry.render(id, variables);
}
