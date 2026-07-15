/**
 * Contribution Manager - Central facade for all plugin contribution registries.
 *
 * Manages specialized per-type registries and handles cross-cutting concerns
 * such as override policy conflict resolution.
 */

import type { BaseFormatter } from "../../services/llm/formatters/base.js";
import type { NodeHandlerFn } from "../../workflow/execution/handlers/node-handlers/index.js";
import type { IToolExecutorConstructor, EventHandler, HookHandler, ContributionType } from "./types.js";
import type { ExecutionMiddleware, MiddlewarePhase } from "./middleware.types.js";
import { OverridePolicy } from "../config.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

// ============================================================
// Internal Registry Types & Classes
// ============================================================

interface NodeTypeEntry {
  handler: NodeHandlerFn;
  pluginId: string;
  template?: Record<string, unknown>;
}

class NodeTypeRegistry {
  private handlers = new Map<string, NodeTypeEntry>();

  register(pluginId: string, nodeType: string, handler: NodeHandlerFn, template?: Record<string, unknown>): void {
    this.handlers.set(nodeType, { handler, pluginId, template });
  }

  getHandler(nodeType: string): NodeHandlerFn | undefined {
    return this.handlers.get(nodeType)?.handler;
  }

  hasHandler(nodeType: string): boolean {
    return this.handlers.has(nodeType);
  }

  getAllNodeTypes(): string[] {
    return Array.from(this.handlers.keys());
  }

  getOwner(nodeType: string): string | undefined {
    return this.handlers.get(nodeType)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [key, value] of this.handlers) {
      if (value.pluginId === pluginId) {
        this.handlers.delete(key);
      }
    }
  }
}

interface ToolTypeEntry {
  executor: IToolExecutorConstructor;
  pluginId: string;
}

class ToolTypeRegistry {
  private executors = new Map<string, ToolTypeEntry>();

  register(pluginId: string, type: string, executor: IToolExecutorConstructor): void {
    this.executors.set(type, { executor, pluginId });
  }

  getExecutor(type: string): IToolExecutorConstructor | undefined {
    return this.executors.get(type)?.executor;
  }

  getAllExecutors(): Map<string, ToolTypeEntry> {
    return new Map(this.executors);
  }

  getAllToolTypes(): string[] {
    return Array.from(this.executors.keys());
  }

  getOwner(type: string): string | undefined {
    return this.executors.get(type)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [key, value] of this.executors) {
      if (value.pluginId === pluginId) {
        this.executors.delete(key);
      }
    }
  }
}

interface LLMProviderEntry {
  formatter: BaseFormatter;
  pluginId: string;
}

class LLMProviderRegistry {
  private providers = new Map<string, LLMProviderEntry>();

  register(pluginId: string, provider: string, formatter: BaseFormatter): void {
    this.providers.set(provider, { formatter, pluginId });
  }

  getFormatter(provider: string): BaseFormatter | undefined {
    return this.providers.get(provider)?.formatter;
  }

  getAllProviders(): string[] {
    return Array.from(this.providers.keys());
  }

  getOwner(provider: string): string | undefined {
    return this.providers.get(provider)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [key, value] of this.providers) {
      if (value.pluginId === pluginId) {
        this.providers.delete(key);
      }
    }
  }
}

interface FormatterEntry {
  formatter: BaseFormatter;
  pluginId: string;
}

class FormatterRegistry {
  private formatters = new Map<string, FormatterEntry>();

  register(pluginId: string, name: string, formatter: BaseFormatter): void {
    this.formatters.set(name, { formatter, pluginId });
  }

  getFormatter(name: string): BaseFormatter | undefined {
    return this.formatters.get(name)?.formatter;
  }

  getOwner(name: string): string | undefined {
    return this.formatters.get(name)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [key, value] of this.formatters) {
      if (value.pluginId === pluginId) {
        this.formatters.delete(key);
      }
    }
  }
}

interface EventHandlerEntry {
  handler: EventHandler;
  pluginId: string;
  priority?: number;
}

class EventHandlerRegistry {
  private handlers = new Map<string, EventHandlerEntry>();

  register(pluginId: string, eventType: string, handler: EventHandler, priority?: number): void {
    this.handlers.set(eventType, { handler, pluginId, priority });
  }

  getAllHandlers(): Map<string, EventHandler> {
    const result = new Map<string, EventHandler>();
    for (const [key, value] of this.handlers) {
      result.set(key, value.handler);
    }
    return result;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [key, value] of this.handlers) {
      if (value.pluginId === pluginId) {
        this.handlers.delete(key);
      }
    }
  }
}

interface HookHandlerEntry {
  handler: HookHandler;
  pluginId: string;
}

class HookHandlerRegistry {
  private handlers = new Map<string, HookHandlerEntry>();

  register(pluginId: string, hookType: string, handler: HookHandler): void {
    this.handlers.set(hookType, { handler, pluginId });
  }

  getAllHandlers(): Map<string, HookHandler> {
    const result = new Map<string, HookHandler>();
    for (const [key, value] of this.handlers) {
      result.set(key, value.handler);
    }
    return result;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [key, value] of this.handlers) {
      if (value.pluginId === pluginId) {
        this.handlers.delete(key);
      }
    }
  }
}

class MiddlewareRegistry {
  private middleware = new Map<MiddlewarePhase, ExecutionMiddleware[]>();

  register(phase: MiddlewarePhase, mw: ExecutionMiddleware): void {
    if (!this.middleware.has(phase)) {
      this.middleware.set(phase, []);
    }
    this.middleware.get(phase)!.push(mw);
    this.middleware.get(phase)!.sort((a, b) => a.priority - b.priority);
  }

  getMiddleware(phase: MiddlewarePhase): ExecutionMiddleware[] {
    return this.middleware.get(phase) || [];
  }

  hasMiddleware(phase: MiddlewarePhase): boolean {
    const mw = this.middleware.get(phase);
    return mw !== undefined && mw.length > 0;
  }

  /**
   * Execute all middleware for a given phase in sequence (onion model).
   * Each middleware calls next() to proceed to the next in the pipeline.
   */
  async runMiddleware(phase: MiddlewarePhase, context: Record<string, unknown>): Promise<void> {
    const mwList = this.middleware.get(phase);
    if (!mwList || mwList.length === 0) return;

    let index = 0;
    const next = async (): Promise<void> => {
      if (index >= mwList.length) return;
      const mw = mwList[index]!;
      index++;
      await mw.handler(context, next);
    };
    await next();
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [phase, mwList] of this.middleware) {
      const filtered = mwList.filter(mw => (mw as unknown as Record<string, unknown>)['_pluginId'] !== pluginId);
      if (filtered.length === 0) {
        this.middleware.delete(phase);
      } else {
        this.middleware.set(phase, filtered);
      }
    }
  }
}

// ============================================================
// Contribution Manager
// ============================================================

/**
 * Contribution Manager - Central facade for all plugin contribution registries.
 */
export class ContributionManager {
  private logger = createContextualLogger({ component: 'ContributionManager' });

  private nodeTypeRegistry = new NodeTypeRegistry();
  private toolTypeRegistry = new ToolTypeRegistry();
  private llmProviderRegistry = new LLMProviderRegistry();
  private formatterRegistry = new FormatterRegistry();
  private eventHandlerRegistry = new EventHandlerRegistry();
  private hookHandlerRegistry = new HookHandlerRegistry();
  private middlewareRegistry = new MiddlewareRegistry();

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

  registerNodeType(
    pluginId: string,
    nodeType: string,
    handler: NodeHandlerFn,
    template?: Record<string, unknown>,
  ): void {
    this.handleConflict('node-type', nodeType, pluginId, () => {
      this.nodeTypeRegistry.register(pluginId, nodeType, handler, template);
    });
  }

  registerToolType(pluginId: string, type: string, executor: IToolExecutorConstructor): void {
    this.handleConflict('tool-type', type, pluginId, () => {
      this.toolTypeRegistry.register(pluginId, type, executor);
    });
  }

  registerLLMProvider(pluginId: string, provider: string, formatter: BaseFormatter): void {
    this.handleConflict('llm-provider', provider, pluginId, () => {
      this.llmProviderRegistry.register(pluginId, provider, formatter);
    });
  }

  registerFormatter(pluginId: string, name: string, formatter: BaseFormatter): void {
    this.handleConflict('formatter', name, pluginId, () => {
      this.formatterRegistry.register(pluginId, name, formatter);
    });
  }

  registerEventHandler(pluginId: string, eventType: string, handler: EventHandler, priority?: number): void {
    this.eventHandlerRegistry.register(pluginId, eventType, handler, priority);
  }

  registerHookHandler(pluginId: string, hookType: string, handler: HookHandler): void {
    this.hookHandlerRegistry.register(pluginId, hookType, handler);
  }

  registerMiddleware(_pluginId: string, phase: MiddlewarePhase, mw: ExecutionMiddleware): void {
    this.middlewareRegistry.register(phase, mw);
  }

  // ============================================================
  // Unregistration
  // ============================================================

  /**
   * Unregister all contributions from a plugin.
   */
  unregisterAll(pluginId: string): void {
    this.nodeTypeRegistry.unregisterByPluginId(pluginId);
    this.toolTypeRegistry.unregisterByPluginId(pluginId);
    this.llmProviderRegistry.unregisterByPluginId(pluginId);
    this.formatterRegistry.unregisterByPluginId(pluginId);
    this.eventHandlerRegistry.unregisterByPluginId(pluginId);
    this.hookHandlerRegistry.unregisterByPluginId(pluginId);
    this.middlewareRegistry.unregisterByPluginId(pluginId);

    this.logger.info(`Unregistered all contributions for plugin: ${pluginId}`);
  }

  // ============================================================
  // Query Methods
  // ============================================================

  getNodeHandler(nodeType: string): NodeHandlerFn | undefined {
    return this.nodeTypeRegistry.getHandler(nodeType);
  }

  getToolExecutor(type: string): IToolExecutorConstructor | undefined {
    return this.toolTypeRegistry.getExecutor(type);
  }

  getAllToolExecutors(): Map<string, { executor: IToolExecutorConstructor; pluginId: string }> {
    return this.toolTypeRegistry.getAllExecutors() as Map<string, { executor: IToolExecutorConstructor; pluginId: string }>;
  }

  getLLMProvider(provider: string): BaseFormatter | undefined {
    return this.llmProviderRegistry.getFormatter(provider);
  }

  getFormatter(name: string): BaseFormatter | undefined {
    return this.formatterRegistry.getFormatter(name);
  }

  hasNodeHandler(nodeType: string): boolean {
    return this.nodeTypeRegistry.hasHandler(nodeType);
  }

  getAllNodeTypes(): string[] {
    return this.nodeTypeRegistry.getAllNodeTypes();
  }

  getAllLLMProviders(): string[] {
    return this.llmProviderRegistry.getAllProviders();
  }

  getAllToolTypes(): string[] {
    return this.toolTypeRegistry.getAllToolTypes();
  }

  getAllEventHandlers(): Map<string, EventHandler> {
    return this.eventHandlerRegistry.getAllHandlers();
  }

  getAllHookHandlers(): Map<string, HookHandler> {
    return this.hookHandlerRegistry.getAllHandlers();
  }

  getMiddleware(phase: MiddlewarePhase): ExecutionMiddleware[] {
    return this.middlewareRegistry.getMiddleware(phase);
  }

  hasMiddleware(phase: MiddlewarePhase): boolean {
    return this.middlewareRegistry.hasMiddleware(phase);
  }

  /**
   * Execute all middleware for a given phase in sequence (onion model).
   * Silently skips if no middleware is registered for the phase.
   */
  async runMiddleware(phase: MiddlewarePhase, context: Record<string, unknown>): Promise<void> {
    await this.middlewareRegistry.runMiddleware(phase, context);
  }

  // ============================================================
  // Conflict Handling
  // ============================================================

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
        case OverridePolicy.PRIORITY:
          registerFn();
          return;
      }
    }
    registerFn();
  }

  private getExistingEntry(type: ContributionType, key: string): string | undefined {
    switch (type) {
      case 'node-type':
        return this.nodeTypeRegistry.getOwner(key);
      case 'tool-type':
        return this.toolTypeRegistry.getOwner(key);
      case 'llm-provider':
        return this.llmProviderRegistry.getOwner(key);
      case 'formatter':
        return this.formatterRegistry.getOwner(key);
      default:
        return undefined;
    }
  }
}