/**
 * FragmentRegistry - System Prompt Fragment Registry
 *
 * Manages system prompt fragments with dependency tracking.
 * Tracks which templates reference each fragment for cascade-aware unregistration.
 *
 * This module only exports class definitions; instances are managed by the DI container as singletons.
 */

import type { SystemPromptFragment } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { RegistryImpl } from "./utils/index.js";
import { renderTemplate } from "../utils/template-renderer/index.js";
import { RegistryAlreadyExistsError } from "./types.js";
import { validateFragment } from "./utils/index.js";

const logger = createContextualLogger({ component: "FragmentRegistry" });

export interface UnregisterResult {
  removed: boolean;
  affectedDependents: string[];
}

/**
 * Fragment Registry Class
 *
 * Manages system prompt fragments with:
 * - Dependency tracking: Records which templates reference each fragment
 * - Validation: Ensures fragment ID and content are non-empty
 * - Unregister with cascade info: Reports affected dependent templates
 *
 * Extends RegistryImpl<SystemPromptFragment> for base CRUD operations.
 */
export class FragmentRegistry extends RegistryImpl<SystemPromptFragment> {
  /** Tracks which templates reference each fragment (fragmentId → Set<templateId>) */
  private dependents = new Map<string, Set<string>>();

  /**
   * Register a fragment with validation.
   *
   * @param key Fragment ID
   * @param fragment The fragment definition
   * @param options Registration options
   * @throws Error if fragment already exists and skipIfExists is not set
   */
  register(key: string, fragment: SystemPromptFragment, options?: { skipIfExists?: boolean }): void {
    if (this.has(key)) {
      if (options?.skipIfExists) {
        return;
      }
      throw new RegistryAlreadyExistsError(key, "Fragment");
    }

    validateFragment(fragment, logger);
    this.set(key, fragment);
  }

  /**
   * Batch register multiple items.
   * @param items Array of items to register
   * @param options Registration options
   */
  registerAll(items: SystemPromptFragment[], options?: { skipIfExists?: boolean }): void {
    for (const item of items) {
      this.register(item.id, item, options);
    }
  }

  /**
   * Batch render multiple fragments with optional variable maps.
   *
   * @param ids Fragment IDs to render
   * @param variablesMap Optional map of fragment ID to variable values
   * @returns Array of rendered content strings (empty strings for missing fragments)
   */
  renderAll(ids: string[], variablesMap?: Map<string, Record<string, unknown>>): string[] {
    return ids.map(id => {
      const fragment = this.get(id);
      if (!fragment) return "";

      const vars = variablesMap?.get(id);
      if (!vars || Object.keys(vars).length === 0) {
        return fragment.content;
      }
      return renderTemplate(fragment.content, vars);
    });
  }

  /**
   * Get fragments by category.
   *
   * @param category Fragment category
   * @returns Array of fragments in the category
   */
  getByCategory(category: SystemPromptFragment["category"]): SystemPromptFragment[] {
    return this.list().filter(f => f.category === category);
  }

  /**
   * Unregister a fragment by ID.
   *
   * @param key Fragment ID to remove
   * @param options Unregister options
   * @returns Whether the fragment was removed
   */
  unregister(key: string, options?: { force?: boolean }): boolean {
    const affectedDependents = this.getDependents(key);
    if (affectedDependents.length > 0 && !options?.force) {
      logger.warn(
        `Fragment '${key}' has dependents: ${affectedDependents.join(", ")}. Use force to unregister.`,
      );
      return false;
    }

    if (affectedDependents.length > 0) {
      logger.warn(
        `Fragment '${key}' unregistered. Affected dependents: ${affectedDependents.join(", ")}`,
      );
    }

    this.dependents.delete(key);
    return this.items.delete(key);
  }

  /**
   * Clear all fragments and dependency tracking.
   */
  override clear(): void {
    super.clear();
    this.dependents.clear();
  }

  /**
   * Record that a template references a fragment.
   *
   * @param fragmentId The fragment ID being referenced
   * @param templateId The template ID that references the fragment
   */
  addDependent(fragmentId: string, templateId: string): void {
    if (!this.dependents.has(fragmentId)) {
      this.dependents.set(fragmentId, new Set());
    }
    this.dependents.get(fragmentId)!.add(templateId);
  }

  /**
   * Get all template IDs that reference a given fragment.
   *
   * @param fragmentId The fragment ID
   * @returns Array of template IDs
   */
  getDependents(fragmentId: string): string[] {
    return Array.from(this.dependents.get(fragmentId) ?? []);
  }
}
