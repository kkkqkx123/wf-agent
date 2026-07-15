/**
 * Plugin Lifecycle Manager - Manages the lifecycle of plugins.
 *
 * Responsibilities:
 * - Load plugins (scan, validate, import)
 * - Activate plugins (call onLoad, register contributions)
 * - Deactivate plugins (call onDeactivate, unregister contributions)
 * - Unload plugins (call onUnload, cleanup)
 * - Graceful shutdown in reverse dependency order
 */

import type { PluginManifest, PluginContext, PluginRecord, PluginSystemOptions } from "./types.js";
import { PluginStatus } from "./types.js";
import { PluginRegistry } from "./registry.js";
import { PluginLoader } from "./loader.js";
import { PluginDependencyResolver } from "./dependency-resolver.js";
import { PluginGuard } from "./guard.js";
import { ContributionManager } from "./contributions/manager.js";
import { ContributionRegistrarImpl } from "./contributions/registrar.js";
import type { ContributionBridge } from "./contributions/bridge.js";
import { PluginEventBus } from "./event-bus.js";
import { OverridePolicy, mergePluginOptions } from "./config.js";
import type { Container } from "@wf-agent/common-utils";
import { createContextualLogger } from "../utils/contextual-logger.js";

/**
 * Plugin Lifecycle Manager
 */
export class PluginLifecycleManager {
  private logger = createContextualLogger({ component: 'PluginLifecycle' });

  constructor(
    private registry: PluginRegistry,
    private loader: PluginLoader,
    private dependencyResolver: PluginDependencyResolver,
    private guard: PluginGuard,
    private contributionManager: ContributionManager,
    private bridge: ContributionBridge | undefined,
    private eventBus: PluginEventBus,
    private container: Container,
    private options: PluginSystemOptions,
    private sdkVersion: string,
  ) {}

  /**
   * Discover and load all plugins from configured paths.
   */
  async discoverAndLoad(): Promise<PluginRecord[]> {
    // 1. Discover plugins
    const discovered = await this.loader.scanPlugins();
    this.logger.info(`Discovered ${discovered.length} plugin(s)`);

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
        this.logger.warn(`Plugin '${d.manifest.id}' manifest validation failed`, { errors: result.errors });
      }
    }

    // 4. Resolve dependencies
    const graph = this.dependencyResolver.resolve(valid);
    if (graph.cycles.length > 0) {
      this.logger.warn(`Detected circular dependencies between plugins`, { cycles: graph.cycles });
    }
    if (graph.missing.length > 0) {
      this.logger.warn(`Missing plugin dependencies`, { missing: graph.missing });
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
        this.logger.info(`Loaded plugin: ${manifest.id}@${manifest.version}`);
      } catch (error) {
        this.logger.error(`Failed to load plugin '${pluginId}'`, undefined, undefined, error instanceof Error ? error : undefined);
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
  async activate(pluginId: string): Promise<void> {
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
      const context: PluginContext = {
        sdkVersion: this.sdkVersion,
        container: this.container,
        logger: createContextualLogger({ component: `Plugin:${pluginId}` }),
        config: pluginConfig,
        contributionManager: this.contributionManager,
        pluginId,
      };

      // Step 1: Call onLoad lifecycle hook first (plugin initialization)
      if (plugin.onLoad) {
        await this.guard.execute(pluginId, () => plugin.onLoad!(context));
      }

      // Step 2: Then register contributions (plugin is initialized)
      if (plugin.registerContributions) {
        const registrar = new ContributionRegistrarImpl(pluginId, this.contributionManager, this.options.overridePolicy || OverridePolicy.FORBID);
        await this.guard.execute(pluginId, async () => {
          plugin.registerContributions!(registrar);
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
      this.logger.info(`Activated plugin: ${pluginId}`);
    } catch (error) {
      this.logger.error(`Failed to activate plugin '${pluginId}'`, undefined, undefined, error instanceof Error ? error : undefined);
      this.registry.setError(pluginId, error instanceof Error ? error : new Error(String(error)));
      this.eventBus.emit('plugin:error', pluginId, { error: error instanceof Error ? error.message : String(error) });
      throw error;
    }
  }

  /**
   * Deactivate a plugin.
   */
  async deactivate(pluginId: string): Promise<void> {
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
        const context: PluginContext = {
          sdkVersion: this.sdkVersion,
          container: this.container,
          logger: createContextualLogger({ component: `Plugin:${pluginId}` }),
          config: {},
          contributionManager: this.contributionManager,
          pluginId,
        };
        await this.guard.execute(pluginId, () => plugin.onDeactivate!(context));
      }

      // Call onUnload lifecycle hook
      if (plugin.onUnload) {
        const context: PluginContext = {
          sdkVersion: this.sdkVersion,
          container: this.container,
          logger: createContextualLogger({ component: `Plugin:${pluginId}` }),
          config: {},
          contributionManager: this.contributionManager,
          pluginId,
        };
        await this.guard.execute(pluginId, () => plugin.onUnload!(context));
      }

      this.eventBus.emit('plugin:deactivated', pluginId);

      this.registry.updateStatus(pluginId, PluginStatus.DEACTIVATED);
      this.logger.info(`Deactivated plugin: ${pluginId}`);
    } catch (error) {
      this.logger.error(`Failed to deactivate plugin '${pluginId}'`, undefined, undefined, error instanceof Error ? error : undefined);
      this.registry.setError(pluginId, error instanceof Error ? error : new Error(String(error)));
    }
  }

  /**
   * Gracefully shut down all plugins in reverse dependency order.
   */
  async shutdownAll(): Promise<void> {
    const activePlugins = this.registry.listByStatus(PluginStatus.ACTIVE);
    const loadedPlugins = this.registry.listByStatus(PluginStatus.LOADED);

    // Deactivate active plugins first (in reverse dependency order)
    const allManifests = [...activePlugins, ...loadedPlugins].map(p => p.manifest);
    const graph = this.dependencyResolver.resolve(allManifests);
    const reverseOrder = [...graph.loadOrder].reverse();

    for (const pluginId of reverseOrder) {
      try {
        await this.deactivate(pluginId);
      } catch (error) {
        this.logger.error(`Error during plugin shutdown '${pluginId}'`, undefined, undefined, error instanceof Error ? error : undefined);
      }
    }

    // Clear the registry
    this.registry.clear();
    this.logger.info('All plugins shut down');
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
}