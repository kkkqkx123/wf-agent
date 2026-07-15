/**
 * Plugin Manager - High-level Plugin Management API for SDK-Kit.
 *
 * Provides a simplified interface for managing plugins:
 * - Load plugins from package names or paths
 * - Activate and deactivate plugins
 * - List and query plugin information
 * - Update plugin configuration at runtime
 */

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
   * Load a plugin from a filesystem path.
   *
   * Loads a single plugin directly from the given path without triggering a full
   * discovery scan. The plugin is validated and registered. If auto-activate is
   * enabled, the plugin is activated after loading.
   *
   * Use `findPlugin(id)` to look up an already-registered plugin by ID.
   */
  async loadPluginFromPath(pluginPath: string): Promise<PluginRecord> {
    try {
      const record = await this.engine.loadSingle(pluginPath);
      if (!record) {
        throw new KitError(
          `No valid plugin found at '${pluginPath}'`,
          KitErrorCode.RESOURCE_NOT_FOUND,
          { pluginPath },
        );
      }

      // Activate if auto-activate is enabled
      if (this.isAutoActivateEnabled()) {
        await this.engine.activate(record.manifest.id);
      }

      return record;
    } catch (error) {
      if (error instanceof KitError) throw error;
      throw new KitError(
        `Failed to load plugin from '${pluginPath}': ${error instanceof Error ? error.message : String(error)}`,
        KitErrorCode.INTERNAL_ERROR,
        { pluginPath },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Find a plugin by ID, name, or entryPoint from the discovered set.
   *
   * Triggers a discovery scan of all configured plugin paths (results are cached
   * to avoid repeated filesystem scans). If the plugin is found and auto-activate
   * is enabled, it is activated.
   *
   * For loading a plugin from a specific filesystem path, use `loadPluginFromPath()`.
   */
  async findPlugin(source: string): Promise<PluginRecord> {
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

      return record;
    } catch (error) {
      if (error instanceof KitError) throw error;
      throw new KitError(
        `Failed to find plugin '${source}': ${error instanceof Error ? error.message : String(error)}`,
        KitErrorCode.INTERNAL_ERROR,
        { source },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Load a plugin — convenience method that delegates to the correct path.
   *
   * If `source` looks like a filesystem path (contains `/` or `\`), it calls
   * `loadPluginFromPath()`; otherwise it calls `findPlugin()` for ID-based lookup.
   *
   * @deprecated Use `loadPluginFromPath(path)` or `findPlugin(id)` explicitly.
   */
  async loadPlugin(source: string): Promise<PluginRecord> {
    if (source.includes('/') || source.includes('\\')) {
      return this.loadPluginFromPath(source);
    }
    return this.findPlugin(source);
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
   * Fully remove a plugin: deactivate, then clear its registry entry.
   * The plugin must be re-discovered and re-loaded to be activated again.
   */
  async unload(pluginId: string): Promise<void> {
    try {
      await this.engine.unload(pluginId);
    } catch (error) {
      throw new KitError(
        `Failed to unload plugin '${pluginId}': ${error instanceof Error ? error.message : String(error)}`,
        KitErrorCode.INTERNAL_ERROR,
        { pluginId },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * List all plugins.
   */
  list(): PluginRecord[] {
    try {
      return this.engine.getPluginRegistry().list();
    } catch (error) {
      return [];
    }
  }

  /**
   * Get plugin details by ID.
   */
  get(pluginId: string): PluginRecord | undefined {
    try {
      return this.engine.getPluginRegistry().get(pluginId);
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
}