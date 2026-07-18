/**
 * Base Contribution Registry - Shared base class for all plugin contribution registries.
 *
 * Provides common infrastructure:
 * - Entry storage with pluginId tracking
 * - getOwner() for conflict detection
 * - unregisterByPluginId() for bulk cleanup
 * - clear() for reset
 *
 * Each specific registry extends this base and adds its own typed register/get methods.
 *
 * @internal - Used internally by ContributionManager.
 */

/**
 * Base entry type all contribution entries must extend.
 */
export interface ContributionEntry {
  pluginId: string;
}

/**
 * Base class for all plugin contribution registries.
 * Provides shared Map storage and common operations.
 */
export abstract class BaseContributionRegistry<TEntry extends ContributionEntry> {
  protected entries = new Map<string, TEntry>();

  /**
   * Get the plugin ID that owns a given key.
   */
  getOwner(key: string): string | undefined {
    return this.entries.get(key)?.pluginId;
  }

  /**
   * Remove all entries contributed by a specific plugin.
   */
  unregisterByPluginId(pluginId: string): void {
    for (const [key, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(key);
      }
    }
  }

  /**
   * Clear all entries.
   */
  clear(): void {
    this.entries.clear();
  }
}