/**
 * Plugin Engine - Central orchestrator for the plugin system.
 *
 * Manages the entire plugin lifecycle:
 * - Discover, load, activate, deactivate, and unload plugins
 * - Coordinates between PluginLoader, PluginRegistry, PluginLifecycleManager,
 *   PluginDependencyResolver, PluginGuard, and ContributionManager
 */

import type { Container } from "@wf-agent/common-utils";
import type { PluginRecord, PluginSystemOptions, PluginEngineOptions } from "./types.js";
import { PluginStatus } from "./types.js";
import { PluginRegistry } from "./registry.js";
import { PluginLoader } from "./loader.js";
import { PluginDependencyResolver } from "./dependency-resolver.js";
import { PluginLifecycleManager } from "./lifecycle.js";
import { PluginGuard } from "./guard.js";
import { ContributionManager } from "./contributions/manager.js";
import { ContributionBridge, type SDKRegistries } from "./contributions/bridge.js";
import { PluginEventBus } from "./event-bus.js";
import { mergePluginOptions, OverridePolicy } from "./config.js";
import { createContextualLogger } from "../utils/contextual-logger.js";

/**
 * Plugin Engine - Central orchestrator for the plugin system.
 */
export class PluginEngine {
  private loader: PluginLoader;
  private registry: PluginRegistry;
  private lifecycle: PluginLifecycleManager;
  private dependencyResolver: PluginDependencyResolver;
  private guard: PluginGuard;
  private contributionManager: ContributionManager;
  private bridge: ContributionBridge | undefined;
  private eventBus: PluginEventBus;
  private options: PluginSystemOptions;
  private logger = createContextualLogger({ component: 'PluginEngine' });
  private initialized: boolean = false;

  /** Cached discover results to avoid repeated filesystem scans. */
  private cachedRecords: PluginRecord[] | null = null;

  constructor(
    private container: Container,
    private engineOptions: PluginEngineOptions,
    registries?: SDKRegistries,
    contributionManager?: ContributionManager,
  ) {
    this.options = mergePluginOptions(engineOptions.plugins);
    this.registry = new PluginRegistry();
    this.dependencyResolver = new PluginDependencyResolver();
    this.guard = new PluginGuard({
      timeout: this.options.guardTimeout ?? 10000,
    });
    this.contributionManager = contributionManager ?? new ContributionManager();
    this.contributionManager.setOverridePolicy(this.options.overridePolicy ?? OverridePolicy.FORBID);
    this.loader = new PluginLoader({ paths: this.options.paths || [] });
    this.eventBus = new PluginEventBus();
    this.bridge = registries
      ? new ContributionBridge(this.contributionManager, registries)
      : undefined;
    this.lifecycle = new PluginLifecycleManager(
      this.registry,
      this.loader,
      this.dependencyResolver,
      this.guard,
      this.contributionManager,
      this.bridge,
      this.eventBus,
      this.container,
      this.options,
      this.engineOptions.sdkVersion,
    );
  }

  /**
   * Discover and load all plugins from configured paths.
   * Results are cached for subsequent calls. Use `forceRescan = true` to bypass cache.
   */
  async discover(forceRescan = false): Promise<PluginRecord[]> {
    if (this.cachedRecords && !forceRescan) {
      return this.cachedRecords;
    }
    const records = await this.lifecycle.discoverAndLoad();
    this.cachedRecords = records;
    return records;
  }

  /**
   * Load a specific plugin by ID.
   */
  async load(pluginId: string): Promise<PluginRecord | undefined> {
    // First, discover all plugins
    const discovered = await this.discover();
    return discovered.find(r => r.manifest.id === pluginId);
  }

  /**
   * Activate a loaded plugin.
   */
  async activate(pluginId: string): Promise<void> {
    await this.lifecycle.activate(pluginId);
  }

  /**
   * Deactivate and unload a plugin.
   */
  async deactivate(pluginId: string): Promise<void> {
    await this.lifecycle.deactivate(pluginId);
  }

  /**
   * Reload a plugin (deactivate, re-discover, re-activate).
   * Useful for hot-reload during development.
   */
  async reload(pluginId: string): Promise<void> {
    this.invalidateCache();
    await this.deactivate(pluginId);
    this.registry.remove(pluginId);

    const discovered = await this.loader.scanPlugins();
    const manifest = discovered.find(d => d.manifest.id === pluginId)?.manifest;
    if (!manifest) {
      throw new Error(`Plugin '${pluginId}' not found during reload`);
    }

    const plugin = await this.loader.loadModule(manifest, true);
    this.registry.register(manifest, plugin);
    await this.activate(pluginId);
  }

  /**
   * Load and activate all discovered plugins.
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      this.logger.warn('PluginEngine already initialized');
      return;
    }

    this.logger.info('Initializing plugin engine...');

    if (!this.options.enabled) {
      this.logger.info('Plugin system is disabled');
      this.initialized = true;
      return;
    }

    try {
      // Discover and load plugins
      const loaded = await this.discover();
      this.logger.info(`Loaded ${loaded.length} plugin(s)`);

      if (this.options.autoActivate !== false) {
        // Activate all loaded plugins
        for (const record of loaded) {
          try {
            await this.activate(record.manifest.id);
          } catch (error) {
            this.logger.error(`Failed to activate plugin '${record.manifest.id}'`, undefined, undefined, error instanceof Error ? error : undefined);
          }
        }

        const activeCount = this.registry.listByStatus(PluginStatus.ACTIVE).length;
        this.logger.info(`Activated ${activeCount} plugin(s)`);
      }

      this.initialized = true;
    } catch (error) {
      this.logger.error('Failed to initialize plugin engine', undefined, undefined, error instanceof Error ? error : undefined);
      throw error;
    }
  }

  /**
   * Gracefully shut down all active plugins.
   */
  async shutdown(): Promise<void> {
    if (!this.initialized) {
      return;
    }

    this.logger.info('Shutting down plugin engine...');
    this.eventBus.emit('plugin:deactivating', '*');
    await this.lifecycle.shutdownAll();
    this.eventBus.removeAll();
    this.initialized = false;
  }

  /**
   * Get the contribution manager for querying contributions.
   */
  getContributionManager(): ContributionManager {
    return this.contributionManager;
  }

  /**
   * Get the plugin registry.
   */
  getPluginRegistry(): PluginRegistry {
    return this.registry;
  }

  /**
   * Get the event bus for subscribing to plugin lifecycle events.
   */
  getEventBus(): PluginEventBus {
    return this.eventBus;
  }

  /**
   * Check if the plugin engine is initialized.
   */
  isInitialized(): boolean {
    return this.initialized;
  }

  /**
   * Get the plugin-specific configuration for a given plugin ID.
   * Returns an empty object if no config is found.
   */
  getPluginConfig(pluginId: string): Record<string, unknown> {
    return this.options.config?.[pluginId] || {};
  }

  /**
   * Update the plugin-specific configuration at runtime.
   * The new config is merged with any existing config for the plugin.
   * Does NOT trigger the plugin's onConfigChange hook — callers should do that separately.
   */
  updatePluginConfig(pluginId: string, config: Record<string, unknown>): void {
    if (!this.options.config) {
      this.options.config = {};
    }
    this.options.config[pluginId] = {
      ...(this.options.config[pluginId] || {}),
      ...config,
    };
  }

  /**
   * Check if auto-activate is enabled in the plugin options.
   */
  isAutoActivateEnabled(): boolean {
    return this.options.autoActivate !== false;
  }

  /**
   * Invalidate the discover cache. Called automatically on reload.
   */
  invalidateCache(): void {
    this.cachedRecords = null;
  }
}