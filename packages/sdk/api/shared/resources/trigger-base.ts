/**
 * BaseTriggerResourceAPI - Shared base class for trigger resource management
 *
 * Provides a unified base implementation for trigger CRUD and query operations.
 * Both Agent and Workflow versions extend this class, providing only entity-specific
 * type annotations and method naming.
 *
 * Entity-specific subclasses must implement:
 * - getAllEntitiesTriggers() - Get all triggers across all entities
 * - getEntityTriggers(entityId) - Get triggers for a specific entity
 * - getEntityTrigger(entityId, triggerId) - Get a specific trigger
 * - enableEntityTrigger(entityId, triggerId) - Enable a trigger
 * - disableEntityTrigger(entityId, triggerId) - Disable a trigger
 * - isEntityTriggerEnabled(entityId, triggerId) - Check if trigger is enabled
 * - getEntityIdForTrigger(trigger) - Get entity ID from trigger
 * - getTriggerId(trigger) - Get trigger ID
 * - getTriggerName(trigger) - Get trigger name for search
 * - getTriggerType(trigger) - Get trigger type for statistics
 * - isTriggerObjectEnabled(trigger) - Check if trigger object is enabled
 */

import { QueryableResourceAPI } from "./generic-resource-api.js";

/**
 * Base trigger filter - entity-specific filters extend this
 */
export interface BaseTriggerFilter {
  /** Filter by trigger IDs */
  ids?: string[];
  /** Filter by name (fuzzy matching) */
  name?: string;
  /** Filter by enabled state */
  enabled?: boolean;
}

/**
 * Trigger statistics
 */
export interface TriggerStatistics {
  /** Total number of triggers */
  total: number;
  /** Number of enabled triggers */
  enabled: number;
  /** Number of disabled triggers */
  disabled: number;
  /** Distribution by trigger type */
  byType: Record<string, number>;
}

/**
 * Global trigger statistics
 */
export interface GlobalTriggerStatistics {
  /** Total number of triggers */
  total: number;
  /** Number of enabled triggers */
  enabled: number;
  /** Number of disabled triggers */
  disabled: number;
  /** Distribution by entity */
  byEntity: Record<string, number>;
  /** Distribution by type */
  byType: Record<string, number>;
}

/**
 * BaseTriggerResourceAPI - Shared base class for trigger resource management
 */
export abstract class BaseTriggerResourceAPI<TTrigger, TFilter extends BaseTriggerFilter> extends QueryableResourceAPI<
  TTrigger,
  string,
  TFilter
> {
  /**
   * Get all triggers across all entities
   */
  protected abstract getAllEntitiesTriggers(): TTrigger[] | Promise<TTrigger[]>;

  /**
   * Get triggers for a specific entity
   * @param entityId Entity ID
   * @returns Array of triggers
   */
  protected abstract getEntityTriggers(entityId: string): TTrigger[] | Promise<TTrigger[]>;

  /**
   * Get a specific trigger for an entity
   * @param entityId Entity ID
   * @param triggerId Trigger ID
   * @returns Trigger or throws if not found
   */
  protected abstract getEntityTrigger(entityId: string, triggerId: string): Promise<TTrigger>;

  /**
   * Enable a trigger
   * @param entityId Entity ID
   * @param triggerId Trigger ID
   */
  protected abstract enableEntityTrigger(entityId: string, triggerId: string): Promise<void>;

  /**
   * Disable a trigger
   * @param entityId Entity ID
   * @param triggerId Trigger ID
   */
  protected abstract disableEntityTrigger(entityId: string, triggerId: string): Promise<void>;

  /**
   * Check if a trigger is enabled
   * @param entityId Entity ID
   * @param triggerId Trigger ID
   * @returns Whether the trigger is enabled
   */
  protected abstract isEntityTriggerEnabled(entityId: string, triggerId: string): Promise<boolean>;

  /**
   * Get the entity ID from a trigger (for global lookup)
   * @param trigger Trigger object
   * @returns Entity ID
   */
  protected abstract getEntityIdForTrigger(trigger: TTrigger): string;

  /**
   * Get the trigger ID from a trigger
   * @param trigger Trigger object
   * @returns Trigger ID
   */
  protected abstract getTriggerId(trigger: TTrigger): string;

  /**
   * Get the trigger name (for search)
   * @param trigger Trigger object
   * @returns Trigger name
   */
  protected abstract getTriggerName(trigger: TTrigger): string;

  /**
   * Get the trigger type (for statistics)
   * @param trigger Trigger object
   * @returns Trigger type string
   */
  protected abstract getTriggerType(trigger: TTrigger): string;

  /**
   * Check whether the trigger object is enabled (for statistics)
   * @param trigger Trigger object
   * @returns Whether enabled
   */
  protected abstract isTriggerObjectEnabled(trigger: TTrigger): boolean;

  /**
   * Get a single trigger by ID across all entities
   */
  protected async getResource(id: string): Promise<TTrigger | null> {
    const allTriggers = await this.getAllEntitiesTriggers();
    return allTriggers.find((t) => this.getTriggerId(t) === id) ?? null;
  }

  /**
   * Get all triggers across all entities
   */
  protected async getAllResources(): Promise<TTrigger[]> {
    return this.getAllEntitiesTriggers();
  }

  /**
   * Apply filter criteria
   */
  protected override applyFilter(triggers: TTrigger[], filter: TFilter): TTrigger[] {
    return triggers.filter((trigger) => {
      if (filter.ids && !filter.ids.includes(this.getTriggerId(trigger))) {
        return false;
      }
      if (filter.name) {
        const name = this.getTriggerName(trigger);
        if (!name.toLowerCase().includes(filter.name.toLowerCase())) {
          return false;
        }
      }
      if (filter.enabled !== undefined) {
        if (this.isTriggerObjectEnabled(trigger) !== filter.enabled) {
          return false;
        }
      }
      return true;
    });
  }

  /**
   * Get triggers for an entity with optional filter
   * @param entityId Entity ID
   * @param filter Optional filter criteria
   * @returns Array of triggers
   */
  protected async getEntityTriggersWithFilter(
    entityId: string,
    filter?: TFilter,
  ): Promise<TTrigger[]> {
    let triggers = await this.getEntityTriggers(entityId);

    if (filter) {
      triggers = this.applyFilter(triggers, filter);
    }

    return triggers;
  }

  /**
   * Enable a trigger (public API)
   * @param entityId Entity ID
   * @param triggerId Trigger ID
   */
  async enableTrigger(entityId: string, triggerId: string): Promise<void> {
    return this.enableEntityTrigger(entityId, triggerId);
  }

  /**
   * Disable a trigger (public API)
   * @param entityId Entity ID
   * @param triggerId Trigger ID
   */
  async disableTrigger(entityId: string, triggerId: string): Promise<void> {
    return this.disableEntityTrigger(entityId, triggerId);
  }

  /**
   * Check if a trigger is enabled (public API)
   * @param entityId Entity ID
   * @param triggerId Trigger ID
   * @returns Whether the trigger is enabled
   */
  async isTriggerEnabled(entityId: string, triggerId: string): Promise<boolean> {
    return this.isEntityTriggerEnabled(entityId, triggerId);
  }

  /**
   * Get trigger statistics for an entity
   * @param entityId Entity ID
   * @returns Statistical information
   */
  protected async getEntityTriggerStatistics(entityId: string): Promise<TriggerStatistics> {
    const triggers = await this.getEntityTriggers(entityId);

    const stats: TriggerStatistics = {
      total: triggers.length,
      enabled: 0,
      disabled: 0,
      byType: {},
    };

    for (const trigger of triggers) {
      if (this.isTriggerObjectEnabled(trigger)) {
        stats.enabled++;
      } else {
        stats.disabled++;
      }

      const type = this.getTriggerType(trigger);
      stats.byType[type] = (stats.byType[type] || 0) + 1;
    }

    return stats;
  }

  /**
   * Get trigger statistics across all entities
   * @returns Global statistics information
   */
  protected async getGlobalTriggerStatistics(): Promise<GlobalTriggerStatistics> {
    const allTriggers = await this.getAllEntitiesTriggers();

    const stats: GlobalTriggerStatistics = {
      total: 0,
      enabled: 0,
      disabled: 0,
      byEntity: {},
      byType: {},
    };

    for (const trigger of allTriggers) {
      stats.total++;
      if (this.isTriggerObjectEnabled(trigger)) {
        stats.enabled++;
      } else {
        stats.disabled++;
      }

      const type = this.getTriggerType(trigger);
      stats.byType[type] = (stats.byType[type] || 0) + 1;

      const entityId = this.getEntityIdForTrigger(trigger);
      stats.byEntity[entityId] = (stats.byEntity[entityId] || 0) + 1;
    }

    return stats;
  }

  /**
   * Search triggers by keyword
   * @param query Search keyword
   * @returns Array of matching triggers
   */
  protected async searchTriggersByKeyword(query: string): Promise<TTrigger[]> {
    const allTriggers = await this.getAllEntitiesTriggers();
    const lowerQuery = query.toLowerCase();
    return allTriggers.filter(
      (trigger) =>
        this.getTriggerName(trigger).toLowerCase().includes(lowerQuery) ||
        this.getTriggerId(trigger).toLowerCase().includes(lowerQuery),
    );
  }

  /**
   * Export triggers for an entity as JSON
   * @param entityId Entity ID
   * @returns JSON string
   */
  protected async exportEntityTriggers(entityId: string): Promise<string> {
    const triggers = await this.getEntityTriggers(entityId);
    return JSON.stringify(triggers, null, 2);
  }
}