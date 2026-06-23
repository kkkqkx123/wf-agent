/**
 * PromptTemplateRegistry - Prompt Template Registry
 *
 * Backward-compatible wrapper for sdk/shared/registry/prompt-template-registry.ts.
 * Maintains the old singleton-based API for existing consumers.
 * New code should use GlobalContext.promptTemplateRegistry instead.
 *
 * @deprecated Import from sdk/shared/registry/prompt-template-registry.ts instead
 */

import { PromptTemplateRegistry as CorePromptTemplateRegistry } from "../../shared/registry/prompt-template-registry.js";
import type { PromptTemplate } from "@wf-agent/types";
import type { FragmentRegistry } from "../../shared/registry/fragment-registry.js";

/**
 * Backward-compatible wrapper that provides the old single-arg register(template) API.
 */
class TemplateRegistryWrapper {
  private core: CorePromptTemplateRegistry;

  constructor(core: CorePromptTemplateRegistry) {
    this.core = core;
  }

  /** Old API: register(template) — single argument */
  register(template: PromptTemplate): void {
    this.core.register(template.id, template);
  }

  registerAll(templates: PromptTemplate[]): void {
    this.core.registerAll(templates);
  }

  get(id: string): PromptTemplate | undefined {
    return this.core.get(id);
  }

  has(id: string): boolean {
    return this.core.has(id);
  }

  getAll(): PromptTemplate[] {
    return this.core.list();
  }

  getByCategory(category: string): PromptTemplate[] {
    return this.core.getByCategory(category);
  }

  unregister(id: string): boolean {
    return this.core.unregister(id);
  }

  clear(): void {
    this.core.clear();
  }

  render(id: string, variables: Record<string, unknown>): string | null {
    return this.core.render(id, variables);
  }

  renderSafe(id: string, variables: Record<string, unknown>, defaultValue: string = ""): string {
    return this.core.renderSafe(id, variables, defaultValue);
  }

  getTemplateIds(): string[] {
    return this.core.getTemplateIds();
  }

  get size(): number {
    return this.core.size;
  }

  isInitialized(): boolean {
    return this.core.isInitialized();
  }

  setFragmentRegistry(registry: FragmentRegistry): void {
    this.core.setFragmentRegistry(registry);
  }
}

/**
 * @deprecated Use GlobalContext.promptTemplateRegistry instead
 */
export const templateRegistry = new TemplateRegistryWrapper(new CorePromptTemplateRegistry());

/**
 * @deprecated Use PromptTemplateRegistry from sdk/shared/registry/ instead
 */
export class PromptTemplateRegistry {
  private static instance: PromptTemplateRegistry | null = null;
  private wrapper: TemplateRegistryWrapper;

  private constructor() {
    this.wrapper = templateRegistry;
  }

  static getInstance(): PromptTemplateRegistry {
    if (!PromptTemplateRegistry.instance) {
      PromptTemplateRegistry.instance = new PromptTemplateRegistry();
    }
    return PromptTemplateRegistry.instance;
  }

  static resetInstance(): void {
    PromptTemplateRegistry.instance = null;
    templateRegistry.clear();
  }

  register(template: PromptTemplate): void {
    this.wrapper.register(template);
  }

  registerAll(templates: PromptTemplate[]): void {
    this.wrapper.registerAll(templates);
  }

  get(id: string): PromptTemplate | undefined {
    return this.wrapper.get(id);
  }

  has(id: string): boolean {
    return this.wrapper.has(id);
  }

  getAll(): PromptTemplate[] {
    return this.wrapper.getAll();
  }

  getByCategory(category: string): PromptTemplate[] {
    return this.wrapper.getByCategory(category);
  }

  unregister(id: string): boolean {
    return this.wrapper.unregister(id);
  }

  clear(): void {
    this.wrapper.clear();
  }

  render(id: string, variables: Record<string, unknown>): string | null {
    return this.wrapper.render(id, variables);
  }

  renderSafe(id: string, variables: Record<string, unknown>, defaultValue: string = ""): string {
    return this.wrapper.renderSafe(id, variables, defaultValue);
  }

  getTemplateIds(): string[] {
    return this.wrapper.getTemplateIds();
  }

  get size(): number {
    return this.wrapper.size;
  }

  isInitialized(): boolean {
    return this.wrapper.isInitialized();
  }

  setFragmentRegistry(registry: FragmentRegistry): void {
    this.wrapper.setFragmentRegistry(registry);
  }
}

/**
 * @deprecated Use templateRegistry.register() directly
 */
export function registerTemplate(template: PromptTemplate): void {
  templateRegistry.register(template);
}

/**
 * @deprecated Use templateRegistry.registerAll() directly
 */
export function registerTemplates(templates: PromptTemplate[]): void {
  templateRegistry.registerAll(templates);
}

/**
 * @deprecated Use templateRegistry.get() directly
 */
export function getTemplate(id: string): PromptTemplate | undefined {
  return templateRegistry.get(id);
}

/**
 * @deprecated Use templateRegistry.render() directly
 */
export function renderTemplateById(
  id: string,
  variables: Record<string, unknown>,
): string | null {
  return templateRegistry.render(id, variables);
}
