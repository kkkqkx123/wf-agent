/**
 * Plugin Adapter
 * Encapsulates SDK-Kit PluginManager API calls for CLI plugin management.
 */

import { BaseAdapter } from "./base-adapter.js";
import { PluginManager } from "@wf-agent/sdk-kit";
import type { PluginRecord } from "@wf-agent/sdk/plugin";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Plugin Adapter
 */
export class PluginAdapter extends BaseAdapter {
  private _pluginManager: PluginManager | null = null;

  /**
   * Get or create the PluginManager instance.
   * The PluginManager constructor validates that pluginEngine is available
   * and throws a clear error if plugins are not enabled in config.
   */
  private getPluginManager(): PluginManager {
    if (!this._pluginManager) {
      this._pluginManager = new PluginManager(this.sdk as any);
    }
    return this._pluginManager;
  }

  /**
   * List all registered plugins
   */
  async listPlugins(): Promise<PluginRecord[]> {
    return this.executeWithErrorHandling(async () => {
      return this.getPluginManager().list();
    }, "List plugins");
  }

  /**
   * Get plugin details by ID
   */
  async getPlugin(pluginId: string): Promise<PluginRecord> {
    return this.executeWithErrorHandling(async () => {
      const plugin = this.getPluginManager().get(pluginId);
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
    return this.executeWithErrorHandling(async () => {
      return await this.getPluginManager().loadPluginFromPath(filePath);
    }, "Load plugin");
  }

  /**
   * Find a plugin by ID/name/entryPoint from configured plugin paths
   */
  async findPlugin(source: string): Promise<PluginRecord> {
    return this.executeWithErrorHandling(async () => {
      return await this.getPluginManager().findPlugin(source);
    }, "Find plugin");
  }

  /**
   * Activate a loaded plugin
   */
  async activatePlugin(pluginId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.getPluginManager().activate(pluginId);
      this.logOperation(`Plugin activated: ${pluginId}`);
    }, "Activate plugin");
  }

  /**
   * Deactivate an active plugin
   */
  async deactivatePlugin(pluginId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.getPluginManager().deactivate(pluginId);
      this.logOperation(`Plugin deactivated: ${pluginId}`);
    }, "Deactivate plugin");
  }

  /**
   * Hot-reload a plugin (deactivate -> re-discover -> re-activate)
   */
  async reloadPlugin(pluginId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.getPluginManager().reload(pluginId);
      this.logOperation(`Plugin reloaded: ${pluginId}`);
    }, "Reload plugin");
  }

  /**
   * Fully remove a plugin from the registry
   */
  async unloadPlugin(pluginId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.getPluginManager().unload(pluginId);
      this.logOperation(`Plugin unloaded: ${pluginId}`);
    }, "Unload plugin");
  }

  /**
   * Get plugin runtime configuration
   */
  async getPluginConfig(pluginId: string): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      return this.getPluginManager().getConfig(pluginId);
    }, "Get plugin config");
  }

  /**
   * Update plugin configuration at runtime
   */
  async updatePluginConfig(pluginId: string, config: Record<string, unknown>): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      await this.getPluginManager().updateConfig(pluginId, config);
      this.logOperation(`Plugin config updated: ${pluginId}`);
    }, "Update plugin config");
  }
}