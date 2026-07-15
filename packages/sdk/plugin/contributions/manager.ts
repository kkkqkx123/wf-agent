/**
 * Contribution Manager - Central manager for all plugin contributions.
 *
 * Responsibilities:
 * - Register contributions from plugins
 * - Unregister contributions when a plugin is deactivated
 * - Query contributions by type
 * - Handle override policies for conflicting contributions
 */

import type { BaseFormatter } from "../../services/llm/formatters/base.js";
import type { NodeHandlerFn } from "../../workflow/execution/handlers/node-handlers/index.js";
import type { IToolExecutorConstructor, EventHandler, HookHandler, ContributionType } from "./types.js";
import type { ExecutionMiddleware, MiddlewarePhase } from "./middleware.types.js";
import { OverridePolicy } from "../config.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

/**
 * Contribution Manager
 */
export class ContributionManager {
  private logger = createContextualLogger({ component: 'ContributionManager' });

  // Node type contributions
  private nodeTypeHandlers: Map<string, { handler: NodeHandlerFn; pluginId: string; template?: Record<string, unknown> }> = new Map();

  // Tool type executor constructors
  private toolTypeExecutors: Map<string, { executor: IToolExecutorConstructor; pluginId: string }> = new Map();

  // LLM provider formatters
  private llmProviders: Map<string, { formatter: BaseFormatter; pluginId: string }> = new Map();

  // Formatter registry
  private formatters: Map<string, { formatter: BaseFormatter; pluginId: string }> = new Map();

  // Event handlers
  private eventHandlers: Map<string, { handler: EventHandler; pluginId: string; priority?: number }> = new Map();

  // Hook handlers
  private hookHandlers: Map<string, { handler: HookHandler; pluginId: string }> = new Map();

  // Middleware pipeline
  private middleware: Map<MiddlewarePhase, ExecutionMiddleware[]> = new Map();

  // Override policy
  private overridePolicy: OverridePolicy = OverridePolicy.FORBID;

  /**
   * Set the override policy for contribution conflicts.
   */
  setOverridePolicy(policy: OverridePolicy): void {
    this.overridePolicy = policy;
  }

  /**
   * Check if a contribution type is supported by this manager.
   */
  supportsType(type: ContributionType): boolean {
    switch (type) {
      case 'node-type':
      case 'tool-type':
      case 'llm-provider':
      case 'formatter':
      case 'event-handler':
      case 'hook-handler':
      case 'middleware':
        return true;
      default:
        return false;
    }
  }

  // ============================================================
  // Registration
  // ============================================================

  /**
   * Register a node type handler.
   */
  registerNodeType(
    pluginId: string,
    nodeType: string,
    handler: NodeHandlerFn,
    template?: Record<string, unknown>,
  ): void {
    this.handleConflict('node-type', nodeType, pluginId, () => {
      this.nodeTypeHandlers.set(nodeType, { handler, pluginId, template });
    });
  }

  /**
   * Register a tool executor type.
   */
  registerToolType(pluginId: string, type: string, executor: IToolExecutorConstructor): void {
    this.handleConflict('tool-type', type, pluginId, () => {
      this.toolTypeExecutors.set(type, { executor, pluginId });
    });
  }

  /**
   * Register an LLM provider formatter.
   */
  registerLLMProvider(pluginId: string, provider: string, formatter: BaseFormatter): void {
    this.handleConflict('llm-provider', provider, pluginId, () => {
      this.llmProviders.set(provider, { formatter, pluginId });
    });
  }

  /**
   * Register a formatter by name.
   */
  registerFormatter(pluginId: string, name: string, formatter: BaseFormatter): void {
    this.handleConflict('formatter', name, pluginId, () => {
      this.formatters.set(name, { formatter, pluginId });
    });
  }

  /**
   * Register an event handler.
   */
  registerEventHandler(pluginId: string, eventType: string, handler: EventHandler, priority?: number): void {
    this.eventHandlers.set(eventType, { handler, pluginId, priority });
  }

  /**
   * Register a hook handler.
   */
  registerHookHandler(_pluginId: string, hookType: string, handler: HookHandler): void {
    this.hookHandlers.set(hookType, { handler, pluginId: _pluginId });
  }

  /**
   * Register middleware for a specific phase.
   */
  registerMiddleware(_pluginId: string, phase: MiddlewarePhase, mw: ExecutionMiddleware): void {
    if (!this.middleware.has(phase)) {
      this.middleware.set(phase, []);
    }
    this.middleware.get(phase)!.push(mw);
    // Sort by priority (lower = earlier)
    this.middleware.get(phase)!.sort((a, b) => a.priority - b.priority);
  }

  // ============================================================
  // Unregistration
  // ============================================================

  /**
   * Unregister all contributions from a plugin.
   */
  unregisterAll(pluginId: string): void {
    // Remove node type handlers
    for (const [key, value] of this.nodeTypeHandlers) {
      if (value.pluginId === pluginId) {
        this.nodeTypeHandlers.delete(key);
      }
    }

    // Remove tool type executors
    for (const [key, value] of this.toolTypeExecutors) {
      if (value.pluginId === pluginId) {
        this.toolTypeExecutors.delete(key);
      }
    }

    // Remove LLM providers
    for (const [key, value] of this.llmProviders) {
      if (value.pluginId === pluginId) {
        this.llmProviders.delete(key);
      }
    }

    // Remove formatters
    for (const [key, value] of this.formatters) {
      if (value.pluginId === pluginId) {
        this.formatters.delete(key);
      }
    }

    // Remove event handlers
    for (const [key, value] of this.eventHandlers) {
      if (value.pluginId === pluginId) {
        this.eventHandlers.delete(key);
      }
    }

    // Remove hook handlers
    for (const [key, value] of this.hookHandlers) {
      if (value.pluginId === pluginId) {
        this.hookHandlers.delete(key);
      }
    }

    // Remove middleware — filter out entries from this plugin
    for (const [phase, mwList] of this.middleware) {
      const filtered = mwList.filter(mw => (mw as unknown as Record<string, unknown>)['_pluginId'] !== pluginId);
      if (filtered.length === 0) {
        this.middleware.delete(phase);
      } else {
        this.middleware.set(phase, filtered);
      }
    }

    this.logger.info(`Unregistered all contributions for plugin: ${pluginId}`);
  }

  // ============================================================
  // Query Methods
  // ============================================================

  /**
   * Get a node handler by node type.
   */
  getNodeHandler(nodeType: string): NodeHandlerFn | undefined {
    return this.nodeTypeHandlers.get(nodeType)?.handler;
  }

  /**
   * Get a tool executor constructor by type.
   */
  getToolExecutor(type: string): IToolExecutorConstructor | undefined {
    return this.toolTypeExecutors.get(type)?.executor;
  }

  /**
   * Get all tool executor entries.
   */
  getAllToolExecutors(): Map<string, { executor: IToolExecutorConstructor; pluginId: string }> {
    return new Map(this.toolTypeExecutors);
  }

  /**
   * Get an LLM provider formatter.
   */
  getLLMProvider(provider: string): BaseFormatter | undefined {
    return this.llmProviders.get(provider)?.formatter;
  }

  /**
   * Get a formatter by name.
   */
  getFormatter(name: string): BaseFormatter | undefined {
    return this.formatters.get(name)?.formatter;
  }

  /**
   * Check if a node type handler exists.
   */
  hasNodeHandler(nodeType: string): boolean {
    return this.nodeTypeHandlers.has(nodeType);
  }

  /**
   * Get all registered node types.
   */
  getAllNodeTypes(): string[] {
    return Array.from(this.nodeTypeHandlers.keys());
  }

  /**
   * Get all registered LLM providers.
   */
  getAllLLMProviders(): string[] {
    return Array.from(this.llmProviders.keys());
  }

  /**
   * Get all registered tool types.
   */
  getAllToolTypes(): string[] {
    return Array.from(this.toolTypeExecutors.keys());
  }

  /**
   * Get all registered event handlers.
   */
  getAllEventHandlers(): Map<string, EventHandler> {
    const handlers = new Map<string, EventHandler>();
    for (const [key, value] of this.eventHandlers) {
      handlers.set(key, value.handler);
    }
    return handlers;
  }

  /**
   * Get all registered hook handlers.
   */
  getAllHookHandlers(): Map<string, HookHandler> {
    const handlers = new Map<string, HookHandler>();
    for (const [key, value] of this.hookHandlers) {
      handlers.set(key, value.handler);
    }
    return handlers;
  }

  /**
   * Get middleware for a specific phase.
   */
  getMiddleware(phase: MiddlewarePhase): ExecutionMiddleware[] {
    return this.middleware.get(phase) || [];
  }

  /**
   * Check if there are any middleware for a given phase.
   */
  hasMiddleware(phase: MiddlewarePhase): boolean {
    const mw = this.middleware.get(phase);
    return mw !== undefined && mw.length > 0;
  }

  // ============================================================
  // Conflict Handling
  // ============================================================

  /**
   * Handle a contribution conflict according to the override policy.
   */
  private handleConflict(
    type: ContributionType,
    key: string,
    pluginId: string,
    registerFn: () => void,
  ): void {
    const existing = this.getExistingEntry(type, key);
    if (existing && existing !== pluginId) {
      switch (this.overridePolicy) {
        case OverridePolicy.FORBID:
          this.logger.warn(
            `Plugin '${pluginId}' cannot override ${type} '${key}' (owned by '${existing}'). Override policy is FORBID.`,
          );
          return;
        case OverridePolicy.WARN:
          this.logger.warn(
            `Plugin '${pluginId}' is overriding ${type} '${key}' (was owned by '${existing}').`,
          );
          registerFn();
          return;
        case OverridePolicy.ALLOW:
          registerFn();
          return;
        case OverridePolicy.PRIORITY:
          // Last registered wins with priority
          registerFn();
          return;
      }
    }
    registerFn();
  }

  /**
   * Get the plugin ID that owns an existing entry, if any.
   */
  private getExistingEntry(type: ContributionType, key: string): string | undefined {
    switch (type) {
      case 'node-type':
        return this.nodeTypeHandlers.get(key)?.pluginId;
      case 'tool-type':
        return this.toolTypeExecutors.get(key)?.pluginId;
      case 'llm-provider':
        return this.llmProviders.get(key)?.pluginId;
      case 'formatter':
        return this.formatters.get(key)?.pluginId;
      default:
        return undefined;
    }
  }
}