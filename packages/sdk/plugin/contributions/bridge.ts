/**
 * Contribution Bridge - Bridges plugin contributions to existing SDK registries.
 *
 * Converts plugin abstractions to SDK internal types and syncs them to the
 * appropriate registries so the execution engine can use plugin-contributed types.
 */

import type { ContributionManager } from "./manager.js";
import type { NodeTemplateRegistry } from "../../shared/registry/node-template-registry.js";
import type { ToolRegistry } from "../../shared/registry/tool-registry.js";
import type { FormatterRegistry } from "../../services/llm/formatters/registry.js";
import type { EventRegistry } from "../../shared/registry/event-registry.js";
import type { HookTemplateRegistry } from "../../shared/registry/hook-template-registry.js";
import type { NodeTemplate } from "@wf-agent/types";
import type { Tool } from "@wf-agent/types";
import type { HookTemplate } from "@wf-agent/types";
import type { BaseEvent } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

// ============================================================
// SDK Registries Interface
// ============================================================

/**
 * The set of SDK registries that the bridge can sync contributions to.
 * All registries are optional — only those available will be synced.
 */
export interface SDKRegistries {
  nodeTemplateRegistry?: NodeTemplateRegistry;
  toolRegistry?: ToolRegistry;
  formatterRegistry?: FormatterRegistry;
  eventRegistry?: EventRegistry;
  hookTemplateRegistry?: HookTemplateRegistry;
}

// ============================================================
// Contribution Bridge
// ============================================================

/**
 * ContributionBridge - Syncs plugin contributions to SDK registries.
 *
 * When a plugin is activated, the bridge reads the plugin's contributions
 * from the ContributionManager and registers them with the appropriate SDK
 * registries. When a plugin is deactivated, the contributions are removed.
 */
export class ContributionBridge {
  private logger = createContextualLogger({ component: 'ContributionBridge' });

  /** Track which contributions were synced per plugin, so unsync knows exactly what to remove. */
  private syncedContributions = new Map<string, Set<string>>();

  /** Track event handler unsubscribe functions per plugin for proper cleanup. */
  private eventUnsubscribers = new Map<string, Array<() => void>>();

  constructor(
    private contributionManager: ContributionManager,
    private registries: SDKRegistries,
  ) {}

  /**
   * Sync all contributions from a plugin to the SDK registries.
   */
  async syncPluginContributions(pluginId: string): Promise<void> {
    const tracked = new Set<string>();

    // Sync node type handlers
    if (this.registries.nodeTemplateRegistry) {
      const nodeTypes = this.contributionManager.getAllNodeTypes();
      for (const nodeType of nodeTypes) {
        try {
          const now = Date.now();
          const template: NodeTemplate = {
            name: nodeType,
            type: 'CUSTOM_NODE' as NodeTemplate['type'],
            config: { pluginId } as NodeTemplate['config'],
            createdAt: now,
            updatedAt: now,
          };
          this.registries.nodeTemplateRegistry.set(nodeType, template);
          tracked.add(`node-type:${nodeType}`);
        } catch (error) {
          this.logger.warn(`Failed to sync node type '${nodeType}' for plugin '${pluginId}'`, { error });
        }
      }
    }

    // Sync tool type executors
    if (this.registries.toolRegistry) {
      const toolExecutors = this.contributionManager.getAllToolExecutors();
      for (const [type, entry] of toolExecutors) {
        try {
          const toolId = `plugin:${pluginId}:${type}`;
          const tool: Tool = {
            id: toolId,
            type: 'BUILTIN',
            description: `Plugin tool: ${type} (from ${pluginId})`,
            parameters: { type: 'object', properties: {}, required: [] },
          };
          this.registries.toolRegistry.register(tool, { skipIfExists: true });
          // Also register the executor constructor for lazy instantiation
          this.registries.toolRegistry.registerPluginExecutor(type, entry.executor, pluginId);
          tracked.add(`tool-type:${type}`);
        } catch (error) {
          this.logger.warn(`Failed to sync tool type '${type}' for plugin '${pluginId}'`, { error });
        }
      }
    }

    // Sync LLM provider formatters
    if (this.registries.formatterRegistry) {
      const providers = this.contributionManager.getAllLLMProviders();
      for (const provider of providers) {
        try {
          const formatter = this.contributionManager.getLLMProvider(provider);
          if (formatter) {
            this.registries.formatterRegistry.register(formatter);
            tracked.add(`llm-provider:${provider}`);
          }
        } catch (error) {
          this.logger.warn(`Failed to sync LLM provider '${provider}' for plugin '${pluginId}'`, { error });
        }
      }
    }

    // Sync event handlers (as global listeners)
    if (this.registries.eventRegistry) {
      const eventHandlers = this.contributionManager.getAllEventHandlers();
      const unsubscribers: Array<() => void> = [];
      for (const [eventType, handler] of eventHandlers) {
        try {
          // EventHandler is compatible with BaseEvent listener
          const listener = handler as unknown as (event: BaseEvent) => void | Promise<void>;
          const unsubscribe = this.registries.eventRegistry.onGlobal(listener);
          unsubscribers.push(unsubscribe);
          tracked.add(`event-handler:${eventType}`);
        } catch (error) {
          this.logger.warn(`Failed to sync event handler '${eventType}' for plugin '${pluginId}'`, { error });
        }
      }
      if (unsubscribers.length > 0) {
        this.eventUnsubscribers.set(pluginId, unsubscribers);
      }
    }

    // Sync hook handlers
    if (this.registries.hookTemplateRegistry) {
      const hookHandlers = this.contributionManager.getAllHookHandlers();
      for (const [hookType] of hookHandlers) {
        try {
          const now = Date.now();
          const template: HookTemplate = {
            name: hookType,
            hook: { hookType: 'BEFORE_EXECUTE', eventName: `plugin:${pluginId}:${hookType}` },
            createdAt: now,
            updatedAt: now,
          };
          this.registries.hookTemplateRegistry.set(hookType, template);
          tracked.add(`hook-handler:${hookType}`);
        } catch (error) {
          this.logger.warn(`Failed to sync hook handler '${hookType}' for plugin '${pluginId}'`, { error });
        }
      }
    }

    // Store the tracked set for cleanup
    this.syncedContributions.set(pluginId, tracked);

    if (tracked.size > 0) {
      this.logger.info(`Synced ${tracked.size} contributions for plugin '${pluginId}'`, { synced: Array.from(tracked) });
    }
  }

  /**
   * Remove all contributions from a plugin from the SDK registries.
   */
  async unsyncPluginContributions(pluginId: string): Promise<void> {
    const tracked = this.syncedContributions.get(pluginId);
    if (!tracked || tracked.size === 0) {
      this.logger.info(`No tracked contributions to unsync for plugin '${pluginId}'`);
      return;
    }

    const removed: string[] = [];

    for (const entry of tracked) {
      if (entry.startsWith('node-type:')) {
        const type = entry.slice('node-type:'.length);
        if (this.registries.nodeTemplateRegistry) {
          this.registries.nodeTemplateRegistry.delete(type);
          removed.push(entry);
        }
      } else if (entry.startsWith('tool-type:')) {
        const type = entry.slice('tool-type:'.length);
        if (this.registries.toolRegistry) {
          const toolId = `plugin:${pluginId}:${type}`;
          try {
            this.registries.toolRegistry.unregister(toolId);
          } catch {
            // Tool may not exist in registry (e.g., already removed by another path)
            this.logger.debug(`Tool '${toolId}' not found during unsync, skipping`);
          }
          removed.push(entry);
        }
      } else if (entry.startsWith('llm-provider:')) {
        const provider = entry.slice('llm-provider:'.length);
        if (this.registries.formatterRegistry) {
          this.registries.formatterRegistry.unregister(provider);
          removed.push(entry);
        }
      } else if (entry.startsWith('event-handler:')) {
        // Use captured unsubscribe functions for proper cleanup
        const unsubscribers = this.eventUnsubscribers.get(pluginId);
        if (unsubscribers) {
          for (const unsubscribe of unsubscribers) {
            try {
              unsubscribe();
            } catch (error) {
              this.logger.warn(`Failed to unsubscribe event handler for plugin '${pluginId}'`, { error });
            }
          }
          this.eventUnsubscribers.delete(pluginId);
        }
        removed.push(entry);
      } else if (entry.startsWith('hook-handler:')) {
        const hookType = entry.slice('hook-handler:'.length);
        if (this.registries.hookTemplateRegistry) {
          this.registries.hookTemplateRegistry.delete(hookType);
          removed.push(entry);
        }
      }
    }

    this.syncedContributions.delete(pluginId);

    this.logger.info(`Unsynchronized ${removed.length} contributions for plugin '${pluginId}'`, { removed });
  }
}

// ============================================================
// Bridge Adapter Functions
// ============================================================

// Adapter functions for converting plugin abstractions to SDK internal types.
// These are used internally by the bridge and can be reused by other components.

/**
 * Create a NodeTemplate for a plugin-contributed node type.
 */
export function createPluginNodeTemplate(
  nodeType: string,
  pluginId: string,
): NodeTemplate {
  const now = Date.now();
  return {
    name: nodeType,
    type: nodeType as NodeTemplate['type'],
    config: { pluginId } as NodeTemplate['config'],
    createdAt: now,
    updatedAt: now,
  };
}

/**
 * Create a Tool definition for a plugin-contributed tool type.
 */
export function createPluginTool(
  type: string,
  pluginId: string,
): Tool {
  return {
    id: `plugin:${pluginId}:${type}`,
    type: 'BUILTIN',
    description: `Plugin tool: ${type} (from ${pluginId})`,
    parameters: { type: 'object', properties: {}, required: [] },
  };
}

/**
 * Create a HookTemplate for a plugin-contributed hook handler.
 */
export function createPluginHookTemplate(
  hookType: string,
  pluginId: string,
): HookTemplate {
  const now = Date.now();
  return {
    name: hookType,
    hook: { hookType: 'BEFORE_EXECUTE', eventName: `plugin:${pluginId}:${hookType}` },
    createdAt: now,
    updatedAt: now,
  };
}

/**
 * Adapter: Plugin EventHandler → BaseEvent listener.
 */
export function adaptEventHandler(
  handler: (event: Record<string, unknown>) => void | Promise<void>,
): (event: BaseEvent) => void | Promise<void> {
  return handler as unknown as (event: BaseEvent) => void | Promise<void>;
}