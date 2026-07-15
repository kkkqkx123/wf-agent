/**
 * Plugin Manager - High-level Plugin Management API for SDK-Kit.
 *
 * Provides a simplified interface for managing plugins:
 * - Load plugins from package names or paths
 * - Activate and deactivate plugins
 * - List and query plugin information
 * - Update plugin configuration at runtime
 */

import type { PluginInfo, ContributionType } from "./types.js";
import type { SDK } from "../types/sdk.types.js";
import type { PluginEngine, PluginRecord } from "@wf-agent/sdk/plugin";
import { KitError, KitErrorCode } from "../converters/error.converter.js";

/**
 * Plugin Manager - High-level API for plugin management.
 */
export class PluginManager {
  private engine: PluginEngine;

  constructor(sdk: SDK) {
    if (!sdk.pluginEngine) {
      throw new KitError(
        'Plugin system is not available. Enable plugins in SDK options.',
        KitErrorCode.PLUGIN_NOT_AVAILABLE,
      );
    }
    this.engine = sdk.pluginEngine;
  }

  /**
   * Load and optionally activate a plugin by source identifier.
   *
   * Discovers all configured plugins, then finds the one matching the given
   * source (by id, name, or entryPoint). If auto-activate is enabled, the
   * plugin is activated after loading.
   *
   * NOTE: This does NOT load a single plugin in isolation — it triggers a full
   * discovery scan of all configured plugin paths (results are cached to avoid
   * repeated filesystem scans). To activate an already-loaded plugin by ID,
   * use `activate(pluginId)` instead.
   */
  async loadPlugin(source: string): Promise<PluginInfo> {
    try {
      const records = await this.engine.discover();

      // Find the matching plugin by source
      const record = records.find(r =>
        r.manifest.id === source ||
        r.manifest.name === source ||
        r.manifest.entryPoint === source,
      );

      if (!record) {
        throw new KitError(
          `Plugin '${source}' not found in any configured plugin path`,
          KitErrorCode.RESOURCE_NOT_FOUND,
          { source },
        );
      }

      // Activate if auto-activate is enabled
      if (this.isAutoActivateEnabled()) {
        await this.engine.activate(record.manifest.id);
      }

      return this.toPluginInfo(record);
    } catch (error) {
      if (error instanceof KitError) throw error;
      throw new KitError(
        `Failed to load plugin '${source}': ${error instanceof Error ? error.message : String(error)}`,
        KitErrorCode.INTERNAL_ERROR,
        { source },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Activate a loaded plugin.
   */
  async activate(pluginId: string): Promise<void> {
    try {
      await this.engine.activate(pluginId);
    } catch (error) {
      throw new KitError(
        `Failed to activate plugin '${pluginId}': ${error instanceof Error ? error.message : String(error)}`,
        KitErrorCode.INTERNAL_ERROR,
        { pluginId },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Deactivate and unload a plugin.
   */
  async deactivate(pluginId: string): Promise<void> {
    try {
      await this.engine.deactivate(pluginId);
    } catch (error) {
      throw new KitError(
        `Failed to deactivate plugin '${pluginId}': ${error instanceof Error ? error.message : String(error)}`,
        KitErrorCode.INTERNAL_ERROR,
        { pluginId },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * List all plugins.
   */
  list(): PluginInfo[] {
    try {
      const records = this.engine.getPluginRegistry().list();
      return records.map(r => this.toPluginInfo(r));
    } catch (error) {
      return [];
    }
  }

  /**
   * Get plugin details by ID.
   */
  get(pluginId: string): PluginInfo | undefined {
    try {
      const record = this.engine.getPluginRegistry().get(pluginId);
      return record ? this.toPluginInfo(record) : undefined;
    } catch {
      return undefined;
    }
  }

  /**
   * Reload a plugin (deactivate + re-discover + re-activate).
   * Useful for hot-reload during development.
   */
  async reload(pluginId: string): Promise<void> {
    try {
      await this.engine.reload(pluginId);
    } catch (error) {
      throw new KitError(
        `Failed to reload plugin '${pluginId}': ${error instanceof Error ? error.message : String(error)}`,
        KitErrorCode.INTERNAL_ERROR,
        { pluginId },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Get plugin configuration.
   */
  getConfig(pluginId: string): Record<string, unknown> {
    return this.engine.getPluginConfig(pluginId);
  }

  /**
   * Update plugin configuration at runtime.
   * Updates both the PluginEngine's stored config and notifies the plugin.
   */
  async updateConfig(pluginId: string, config: Record<string, unknown>): Promise<void> {
    try {
      const record = this.engine.getPluginRegistry().get(pluginId);
      if (!record) {
        throw new KitError(
          `Plugin '${pluginId}' not found`,
          KitErrorCode.RESOURCE_NOT_FOUND,
          { pluginId },
        );
      }

      // Update stored config in the engine so getPluginConfig() returns the new value
      this.engine.updatePluginConfig(pluginId, config);

      // Notify the plugin via onConfigChange lifecycle hook
      if (record.instance.onConfigChange) {
        await record.instance.onConfigChange(config);
      }
    } catch (error) {
      if (error instanceof KitError) throw error;
      throw new KitError(
        `Failed to update config for plugin '${pluginId}': ${error instanceof Error ? error.message : String(error)}`,
        KitErrorCode.INTERNAL_ERROR,
        { pluginId },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Check if auto-activate is enabled.
   */
  private isAutoActivateEnabled(): boolean {
    return this.engine.isAutoActivateEnabled();
  }

  /**
   * Convert a PluginRecord to a PluginInfo object.
   */
  private toPluginInfo(record: PluginRecord): PluginInfo {
    return {
      id: record.manifest.id,
      version: record.manifest.version,
      name: record.manifest.name || record.manifest.id,
      status: record.status,
      contributions: (record.manifest.contributions || []) as ContributionType[],
      error: record.error?.message,
      activatedAt: record.activatedAt,
    };
  }
}