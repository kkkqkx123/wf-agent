/**
 * Plugin Engine - Central orchestrator for the plugin system.
 *
 * Manages the entire plugin lifecycle:
 * - Discover, load, activate, deactivate, and unload plugins
 * - Coordinates between PluginLoader, PluginRegistry,
 *   PluginDependencyResolver, PluginGuard, and ContributionManager
 */

import type { PluginManifest, PluginContext, PluginRecord, PluginSystemOptions, PluginEngineOptions } from "./types.js";
import { PluginStatus } from "./types.js";
import { PluginRegistry } from "./registry.js";
import { PluginLoader } from "./loader.js";
import { PluginDependencyResolver } from "./dependency-resolver.js";
import { PluginGuard } from "./guard.js";
import { ContributionManager } from "./contributions/manager.js";
import { ContributionBridge, type SDKRegistries } from "./contributions/bridge.js";
import { PluginEventBus } from "./event-bus.js";
import { OverridePolicy, mergePluginOptions } from "./config.js";
import type { Container } from "@wf-agent/common-utils";
import { createContextualLogger } from "../utils/contextual-logger.js";

/**
 * Plugin Engine - Central orchestrator for the plugin system.
 */
export class PluginEngine {
  private logger = createContextualLogger({ component: 'PluginEngine' });
  private lifecycleLogger = createContextualLogger({ component: 'PluginLifecycle' });

  private loader: PluginLoader;
  private registry: PluginRegistry;
  private dependencyResolver: PluginDependencyResolver;
  private guard: PluginGuard;
  private contributionManager: ContributionManager;
  private bridge: ContributionBridge | undefined;
  private eventBus: PluginEventBus;
  private options: PluginSystemOptions;
  private sdkVersion: string;
  private container: Container;
  private initialized: boolean = false;

  /** Cached discover results to avoid repeated filesystem scans. */
  private cachedRecords: PluginRecord[] | null = null;

  constructor(
    container: Container,
    engineOptions: PluginEngineOptions,
    registries?: SDKRegistries,
    contributionManager?: ContributionManager,
  ) {
    this.container = container;
    this.options = mergePluginOptions(engineOptions.plugins);
    this.sdkVersion = engineOptions.sdkVersion;
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
  }

  // ============================================================
  // Public API
  // ============================================================

  /**
   * Discover and load all plugins from configured paths.
   * Results are cached for subsequent calls. Use `forceRescan = true` to bypass cache.
   */
  async discover(forceRescan = false): Promise<PluginRecord[]> {
    if (this.cachedRecords && !forceRescan) {
      return this.cachedRecords;
    }
    const records = await this.discoverAndLoad();
    this.cachedRecords = records;
    return records;
  }

  /**
   * Activate a loaded plugin.
   */
  async activate(pluginId: string): Promise<void> {
    await this.activatePlugin(pluginId);
  }

  /**
   * Load a single plugin from a filesystem path, bypassing the full discovery scan.
   * The plugin is validated and registered but NOT activated. Call activate() separately.
   * Returns the loaded plugin record, or undefined if the path is invalid.
   */
  async loadSingle(pluginPath: string): Promise<PluginRecord | undefined> {
    const discovered = await this.loader.scanSinglePlugin(pluginPath);
    if (!discovered) {
      return undefined;
    }

    const manifest = discovered.manifest;
    const result = this.loader.validateManifest(manifest, this.sdkVersion);
    if (!result.valid) {
      this.logger.warn(`Plugin '${manifest.id}' manifest validation failed`, { errors: result.errors });
      return undefined;
    }

    try {
      const plugin = await this.loader.loadModule(manifest);
      const record = this.registry.register(manifest, plugin);
      this.registry.updateStatus(manifest.id, PluginStatus.LOADED);
      this.logger.info(`Loaded plugin: ${manifest.id}@${manifest.version}`);
      return record;
    } catch (error) {
      this.logger.error(`Failed to load plugin '${manifest.id}'`, undefined, undefined, error instanceof Error ? error : undefined);
      this.registry.setError(manifest.id, error instanceof Error ? error : new Error(String(error)));
      return undefined;
    }
  }

  /**
   * Deactivate and unload a plugin.
   */
  async deactivate(pluginId: string): Promise<void> {
    await this.deactivatePlugin(pluginId);
  }

  /**
   * Fully remove a plugin: deactivate, then remove from the registry.
   * Unlike deactivate(), this also clears the registry entry so the plugin
   * must be re-discovered and re-loaded before it can be activated again.
   */
  async unload(pluginId: string): Promise<void> {
    await this.deactivatePlugin(pluginId);
    this.registry.remove(pluginId);
  }

  /**
   * Reload a plugin (deactivate, re-discover, re-activate).
   * Useful for hot-reload during development.
   */
  async reload(pluginId: string): Promise<void> {
    this.invalidateCache();
    await this.deactivatePlugin(pluginId);
    this.registry.remove(pluginId);

    await this.reloadSingle(pluginId);
    await this.activatePlugin(pluginId);
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
            await this.activatePlugin(record.manifest.id);
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
    await this.shutdownAll();
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

  // ============================================================
  // Internal Lifecycle Methods
  // ============================================================

  /**
   * Discover and load all plugins from configured paths.
   */
  private async discoverAndLoad(): Promise<PluginRecord[]> {
    // 1. Discover plugins
    const discovered = await this.loader.scanPlugins();
    this.lifecycleLogger.info(`Discovered ${discovered.length} plugin(s)`);

    for (const d of discovered) {
      this.eventBus.emit('plugin:discovered', d.manifest.id);
    }

    if (discovered.length === 0) {
      return [];
    }

    // 2. Filter by allowlist/blocklist
    const filtered = this.filterPlugins(discovered);

    // 3. Validate manifests
    const valid: PluginManifest[] = [];
    for (const d of filtered) {
      const result = this.loader.validateManifest(d.manifest, this.sdkVersion);
      if (result.valid) {
        valid.push(d.manifest);
      } else {
        this.lifecycleLogger.warn(`Plugin '${d.manifest.id}' manifest validation failed`, { errors: result.errors });
      }
    }

    // 4. Resolve dependencies
    const graph = this.dependencyResolver.resolve(valid);
    if (graph.cycles.length > 0) {
      this.lifecycleLogger.warn(`Detected circular dependencies between plugins`, { cycles: graph.cycles });
    }
    if (graph.missing.length > 0) {
      this.lifecycleLogger.warn(`Missing plugin dependencies`, { missing: graph.missing });
    }

    // 5. Load plugins in dependency order
    const loaded: PluginRecord[] = [];
    for (const pluginId of graph.loadOrder) {
      const manifest = valid.find(m => m.id === pluginId);
      if (!manifest) continue;

      try {
        this.eventBus.emit('plugin:loading', pluginId);
        const plugin = await this.guard.execute(pluginId, () =>
          this.loader.loadModule(manifest),
        );
        const record = this.registry.register(manifest, plugin);
        this.registry.updateStatus(pluginId, PluginStatus.LOADED);
        loaded.push(record);
        this.eventBus.emit('plugin:loaded', pluginId, { version: manifest.version });
        this.lifecycleLogger.info(`Loaded plugin: ${manifest.id}@${manifest.version}`);
      } catch (error) {
        this.lifecycleLogger.error(`Failed to load plugin '${pluginId}'`, undefined, undefined, error instanceof Error ? error : undefined);
        this.registry.setError(pluginId, error instanceof Error ? error : new Error(String(error)));
        this.eventBus.emit('plugin:error', pluginId, { error: error instanceof Error ? error.message : String(error) });
      }
    }

    return loaded;
  }

  /**
   * Activate a loaded plugin.
   *
   * Lifecycle order (fixed):
   * 1. onLoad — plugin initialization (setup, config, etc.)
   * 2. registerContributions — declare contributions
   * 3. bridge.sync - sync to SDK registries
   * 4. onActivate — post-registration setup
   */
  private async activatePlugin(pluginId: string): Promise<void> {
    const record = this.registry.get(pluginId);
    if (!record) {
      throw new Error(`Plugin '${pluginId}' not found in registry`);
    }

    if (record.status !== PluginStatus.LOADED) {
      throw new Error(
        `Plugin '${pluginId}' is in '${record.status}' state, expected 'loaded'`,
      );
    }

    this.registry.updateStatus(pluginId, PluginStatus.ACTIVATING);

    try {
      this.eventBus.emit('plugin:activating', pluginId);
      const plugin = record.instance;
      const mergedOptions = mergePluginOptions(this.options);
      const pluginConfig = mergedOptions.config?.[pluginId] || {};

      // Create the plugin context
      const context = this.createPluginContext(pluginId, pluginConfig);

      // Step 1: Call onLoad lifecycle hook first (plugin initialization)
      if (plugin.onLoad) {
        await this.guard.execute(pluginId, () => plugin.onLoad!(context));
      }

      // Step 2: Then register contributions (plugin is initialized)
      if (plugin.registerContributions) {
        this.contributionManager.setPluginContext(pluginId, this.options.overridePolicy || OverridePolicy.FORBID);
        await this.guard.execute(pluginId, async () => {
          plugin.registerContributions!(this.contributionManager);
        });
      }

      // Step 3: Sync contributions to SDK registries via bridge
      if (this.bridge) {
        await this.bridge.syncPluginContributions(pluginId);
      }

      // Step 4: Call onActivate lifecycle hook (post-registration setup)
      if (plugin.onActivate) {
        await this.guard.execute(pluginId, () => plugin.onActivate!(context));
      }

      this.eventBus.emit('plugin:activated', pluginId, { activatedAt: Date.now() });

      this.registry.updateStatus(pluginId, PluginStatus.ACTIVE);
      this.registry.setActivatedAt(pluginId, Date.now());
      this.lifecycleLogger.info(`Activated plugin: ${pluginId}`);
    } catch (error) {
      this.lifecycleLogger.error(`Failed to activate plugin '${pluginId}'`, undefined, undefined, error instanceof Error ? error : undefined);
      this.registry.setError(pluginId, error instanceof Error ? error : new Error(String(error)));
      this.eventBus.emit('plugin:error', pluginId, { error: error instanceof Error ? error.message : String(error) });
      throw error;
    }
  }

  /**
   * Deactivate a plugin.
   */
  private async deactivatePlugin(pluginId: string): Promise<void> {
    const record = this.registry.get(pluginId);
    if (!record) return;

    this.registry.updateStatus(pluginId, PluginStatus.DEACTIVATING);
    this.eventBus.emit('plugin:deactivating', pluginId);

    try {
      const plugin = record.instance;

      // First, unsync contributions from SDK registries via bridge
      if (this.bridge) {
        await this.bridge.unsyncPluginContributions(pluginId);
      }

      // Unregister contributions from the contribution manager
      this.contributionManager.unregisterAll(pluginId);

      // Call onDeactivate lifecycle hook
      if (plugin.onDeactivate) {
        const context = this.createPluginContext(pluginId, {});
        await this.guard.execute(pluginId, () => plugin.onDeactivate!(context));
      }

      // Call onUnload lifecycle hook
      if (plugin.onUnload) {
        const context = this.createPluginContext(pluginId, {});
        await this.guard.execute(pluginId, () => plugin.onUnload!(context));
      }

      this.eventBus.emit('plugin:deactivated', pluginId);

      this.registry.updateStatus(pluginId, PluginStatus.DEACTIVATED);
      this.lifecycleLogger.info(`Deactivated plugin: ${pluginId}`);
    } catch (error) {
      this.lifecycleLogger.error(`Failed to deactivate plugin '${pluginId}'`, undefined, undefined, error instanceof Error ? error : undefined);
      this.registry.setError(pluginId, error instanceof Error ? error : new Error(String(error)));
    }
  }

  /**
   * Reload a single plugin (hot-reload) with full lifecycle validation.
   *
   * Unlike deactivate()+activate(), this also re-scans the filesystem and
   * re-imports the module. All manifest validation, allowList/blockList
   * filtering, and event notifications are applied.
   */
  private async reloadSingle(pluginId: string): Promise<PluginRecord> {
    const discovered = await this.loader.scanPlugins();
    const d = discovered.find(p => p.manifest.id === pluginId);
    if (!d) {
      throw new Error(`Plugin '${pluginId}' not found during reload`);
    }

    // Apply allowList/blockList filtering
    const filtered = this.filterPlugins([d]);
    if (filtered.length === 0) {
      throw new Error(`Plugin '${pluginId}' is blocked by allowList/blockList`);
    }

    // Validate manifest
    const result = this.loader.validateManifest(d.manifest, this.sdkVersion);
    if (!result.valid) {
      throw new Error(
        `Plugin '${pluginId}' manifest validation failed: ${result.errors.join(', ')}`,
      );
    }

    // Emit and load
    this.eventBus.emit('plugin:loading', pluginId);
    const plugin = await this.guard.execute(pluginId, () =>
      this.loader.loadModule(d.manifest, true),
    );
    const record = this.registry.register(d.manifest, plugin);
    this.registry.updateStatus(pluginId, PluginStatus.LOADED);
    this.eventBus.emit('plugin:loaded', pluginId, { version: d.manifest.version });
    this.lifecycleLogger.info(`Reloaded plugin: ${d.manifest.id}@${d.manifest.version}`);
    return record;
  }

  /**
   * Gracefully shut down all plugins in reverse dependency order.
   */
  private async shutdownAll(): Promise<void> {
    const activePlugins = this.registry.listByStatus(PluginStatus.ACTIVE);
    const loadedPlugins = this.registry.listByStatus(PluginStatus.LOADED);

    // Deactivate active plugins first (in reverse dependency order)
    const allManifests = [...activePlugins, ...loadedPlugins].map(p => p.manifest);
    const graph = this.dependencyResolver.resolve(allManifests);
    const reverseOrder = [...graph.loadOrder].reverse();

    for (const pluginId of reverseOrder) {
      try {
        await this.deactivatePlugin(pluginId);
      } catch (error) {
        this.lifecycleLogger.error(`Error during plugin shutdown '${pluginId}'`, undefined, undefined, error instanceof Error ? error : undefined);
      }
    }

    // Clear the registry
    this.registry.clear();
    this.lifecycleLogger.info('All plugins shut down');
  }

  /**
   * Filter discovered plugins by allowlist/blocklist.
   */
  private filterPlugins(discovered: { manifest: PluginManifest }[]): { manifest: PluginManifest }[] {
    const { allowList, blockList } = this.options;

    if (allowList && allowList.length > 0) {
      // Only allow listed plugins
      return discovered.filter(d => allowList.includes(d.manifest.id));
    }

    if (blockList && blockList.length > 0) {
      // Exclude blocklisted plugins
      return discovered.filter(d => !blockList.includes(d.manifest.id));
    }

    return discovered;
  }

  /**
   * Create a PluginContext for a plugin lifecycle hook.
   */
  private createPluginContext(pluginId: string, config: Record<string, unknown> = {}): PluginContext {
    return {
      sdkVersion: this.sdkVersion,
      container: this.container,
      logger: createContextualLogger({ component: `Plugin:${pluginId}` }),
      config,
      contributionManager: this.contributionManager,
      pluginId,
    };
  }
}