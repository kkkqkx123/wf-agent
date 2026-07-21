/**
 * Agent Template Registry
 *
 * Responsible for the registration, querying, and management of agent templates.
 * Agent templates are complete agent definition configurations that can be
 * cloned and reused across executions.
 *
 * This is an Agent-specific feature with no workflow equivalent.
 * At minimum, implements in-memory storage for CRUD operations.
 */

import type { AgentTemplate } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { RegistryImpl } from "../../shared/registry/utils/index.js";

const logger = createContextualLogger({ component: "AgentTemplateRegistry" });

/**
 * Agent Template Registry
 *
 * Manages agent template definitions in memory.
 * Supports CRUD operations and usage tracking.
 */
export class AgentTemplateRegistry extends RegistryImpl<AgentTemplate> {
  /**
   * Register a new agent template
   * @param template Agent template to register
   */
  register(template: AgentTemplate): void {
    const key = template.id;
    if (this.items.has(key)) {
      logger.warn("Overwriting existing agent template", { templateId: key });
    }
    this.items.set(key, template);
    logger.debug("Agent template registered", { templateId: key });
  }

  /**
   * Unregister an agent template
   * @param id Template ID
   * @returns true if the template was removed
   */
  unregister(id: string): boolean {
    const existed = this.items.delete(id);
    if (existed) {
      logger.debug("Agent template unregistered", { templateId: id });
    }
    return existed;
  }

  /**
   * Update an existing agent template
   * @param id Template ID
   * @param updates Partial update fields
   */
  update(id: string, updates: Partial<AgentTemplate>): void {
    const existing = this.items.get(id);
    if (!existing) {
      logger.warn("Cannot update non-existent agent template", { templateId: id });
      return;
    }
    this.items.set(id, { ...existing, ...updates });
  }

  /**
   * Increment the usage count for a template
   * @param id Template ID
   */
  incrementUsageCount(id: string): void {
    const existing = this.items.get(id);
    if (existing) {
      existing.usageCount = (existing.usageCount || 0) + 1;
      this.items.set(id, existing);
    }
  }

  /**
   * Get all registered templates
   * @returns Array of agent templates
   */
  getAll(): AgentTemplate[] {
    return this.list();
  }

  /**
   * Clear all templates
   */
  override clear(): void {
    super.clear();
    logger.debug("All agent templates cleared");
  }
}