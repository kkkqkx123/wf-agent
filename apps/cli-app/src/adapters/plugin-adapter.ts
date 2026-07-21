/**
 * Plugin Adapter
 * Encapsulates SDK-Kit PluginManager API calls for CLI plugin management.
 */

import { BaseAdapter } from "./base-adapter.js";
import { PluginManager } from "@wf-agent/sdk-kit";
import type { PluginRecord } from "@wf-agent/sdk/plugin";
import { CLINotFoundError } from "../types/cli-types.js";
import { KitError } from "@wf-agent/sdk-kit";

/**
 * Plugin Adapter
 */
export class PluginAdapter extends BaseAdapter {
  private _pluginManager: PluginManager | null = null;
  private _pluginManagerError: Error | null = null;

  /**
   * Get or create the PluginManager instance.
   * Returns null if the plugin system is not available, so callers can
   * degrade gracefully (e.g. return an empty list for `plugin list`).
   */
  private getPluginManager(): PluginManager | null {
    if (this._pluginManagerError) return null;
    if (!this._pluginManager) {
      try {
        this._pluginManager = new PluginManager(this.sdk as any);
      } catch (error) {
        if (error instanceof KitError) {
          this._pluginManagerError = error as Error;
          return null;
        }
        throw error;
      }
    }
    return this._pluginManager;
  }

  /**
   * List all registered plugins
   * Returns an empty array when the plugin system is not available.
   */
  async listPlugins(): Promise<PluginRecord[]> {
    const pm = this.getPluginManager();
    if (!pm) return [];
    return this.executeWithErrorHandling(async () => {
      return pm.list();
    }, "List plugins");
  }

  /**
   * Get plugin details by ID
   */
  async getPlugin(pluginId: string): Promise<PluginRecord> {
    const pm = this.getPluginManager();
    if (!pm) {
      throw new CLINotFoundError(`Plugin not found: ${pluginId}`, "Plugin", pluginId);
    }
    return this.executeWithErrorHandling(async () => {
      const plugin = pm.get(pluginId);
      if (!plugin) {
        throw new CLINotFoundError(`Plugin not found: ${pluginId}`, "Plugin", pluginId);
      }
      return plugin;
    }, "Get plugin");
  }

  /**
   * Load a plugin from a filesystem path
   */
  async loadPlugin(filePath: string): Promise<PluginRecord> {
    const pm = this.getPluginManager();
    if (!pm) {
      throw new Error(`Plugin system is not available. Enable plugins in SDK options.`);
    }
    return this.executeWithErrorHandling(async () => {
      return await pm.loadPluginFromPath(filePath);
    }, "Load plugin");
  }

  /**
   * Find a plugin by ID/name/entryPoint from configured plugin paths
   */
  async findPlugin(source: string): Promise<PluginRecord> {
    const pm = this.getPluginManager();
    if (!pm) {
      throw new CLINotFoundError(`Plugin not found: ${source}`, "Plugin", source);
    }
    return this.executeWithErrorHandling(async () => {
      return await pm.findPlugin(source);
    }, "Find plugin");
  }

  /**
   * Activate a loaded plugin
   */
  async activatePlugin(pluginId: string): Promise<void> {
    const pm = this.getPluginManager();
    if (!pm) {
      throw new CLINotFoundError(`Plugin not found: ${pluginId}`, "Plugin", pluginId);
    }
    return this.executeWithErrorHandling(async () => {
      await pm.activate(pluginId);
      this.logOperation(`Plugin activated: ${pluginId}`);
    }, "Activate plugin");
  }

  /**
   * Deactivate an active plugin
   */
  async deactivatePlugin(pluginId: string): Promise<void> {
    const pm = this.getPluginManager();
    if (!pm) {
      throw new CLINotFoundError(`Plugin not found: ${pluginId}`, "Plugin", pluginId);
    }
    return this.executeWithErrorHandling(async () => {
      await pm.deactivate(pluginId);
      this.logOperation(`Plugin deactivated: ${pluginId}`);
    }, "Deactivate plugin");
  }

  /**
   * Hot-reload a plugin (deactivate -> re-discover -> re-activate)
   */
  async reloadPlugin(pluginId: string): Promise<void> {
    const pm = this.getPluginManager();
    if (!pm) {
      throw new CLINotFoundError(`Plugin not found: ${pluginId}`, "Plugin", pluginId);
    }
    return this.executeWithErrorHandling(async () => {
      await pm.reload(pluginId);
      this.logOperation(`Plugin reloaded: ${pluginId}`);
    }, "Reload plugin");
  }

  /**
   * Fully remove a plugin from the registry
   */
  async unloadPlugin(pluginId: string): Promise<void> {
    const pm = this.getPluginManager();
    if (!pm) {
      throw new CLINotFoundError(`Plugin not found: ${pluginId}`, "Plugin", pluginId);
    }
    return this.executeWithErrorHandling(async () => {
      await pm.unload(pluginId);
      this.logOperation(`Plugin unloaded: ${pluginId}`);
    }, "Unload plugin");
  }

  /**
   * Get plugin runtime configuration
   */
  async getPluginConfig(pluginId: string): Promise<Record<string, unknown>> {
    const pm = this.getPluginManager();
    if (!pm) {
      throw new CLINotFoundError(`Plugin not found: ${pluginId}`, "Plugin", pluginId);
    }
    return this.executeWithErrorHandling(async () => {
      return pm.getConfig(pluginId);
    }, "Get plugin config");
  }

  /**
   * Update plugin configuration at runtime
   */
  async updatePluginConfig(pluginId: string, config: Record<string, unknown>): Promise<void> {
    const pm = this.getPluginManager();
    if (!pm) {
      throw new CLINotFoundError(`Plugin not found: ${pluginId}`, "Plugin", pluginId);
    }
    return this.executeWithErrorHandling(async () => {
      await pm.updateConfig(pluginId, config);
      this.logOperation(`Plugin config updated: ${pluginId}`);
    }, "Update plugin config");
  }
}