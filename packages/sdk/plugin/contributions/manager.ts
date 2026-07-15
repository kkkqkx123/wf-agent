/**
 * Contribution Manager - Central facade for all plugin contribution registries.
 *
 * Manages specialized per-type registries and handles cross-cutting concerns
 * such as override policy conflict resolution. Implements ContributionRegistrar
 * directly, eliminating the need for separate adapter classes.
 */

import type { BaseFormatter } from "../../services/llm/formatters/base.js";
import type { NodeHandlerFn } from "../../workflow/execution/handlers/node-handlers/index.js";
import type { IToolExecutorConstructor, EventHandler, HookHandler, ContributionType } from "./types.js";
import type { ExecutionMiddleware, MiddlewarePhase } from "./middleware.types.js";
import type {
  PluginNodeHandler,
  PluginToolExecutor,
  PluginLLMFormatter,
  PluginEventHandler,
  PluginHookHandler,
  PluginExecutionMiddleware,
  PluginLLMRequest,
  PluginLLMResponse,
} from "./abstractions.js";
import type {
  NodeTypeRegistrar,
  ToolTypeRegistrar,
  LLMProviderRegistrar,
  FormatterRegistrar,
  EventHandlerRegistrar,
  HookHandlerRegistrar,
  MiddlewareRegistrar,
  ContributionRegistrar,
} from "./registration.js";
import { OverridePolicy } from "../config.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { IToolExecutor } from "../../services/tools/core/interfaces.js";
import type { Tool, ToolExecutionOptions, ToolExecutionResult } from "@wf-agent/types";

// ============================================================
// Internal Registry Implementations
// ============================================================

interface NodeTypeEntry {
  pluginId: string;
  handler: NodeHandlerFn;
  template?: Record<string, unknown>;
}

class NodeTypeRegistry {
  private entries = new Map<string, NodeTypeEntry>();

  register(pluginId: string, nodeType: string, handler: NodeHandlerFn, template?: Record<string, unknown>): void {
    this.entries.set(nodeType, { pluginId, handler, template });
  }

  getHandler(nodeType: string): NodeHandlerFn | undefined {
    return this.entries.get(nodeType)?.handler;
  }

  hasHandler(nodeType: string): boolean {
    return this.entries.has(nodeType);
  }

  getAllNodeTypes(): string[] {
    return Array.from(this.entries.keys());
  }

  getOwner(nodeType: string): string | undefined {
    return this.entries.get(nodeType)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [nodeType, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(nodeType);
      }
    }
  }
}

interface ToolTypeEntry {
  pluginId: string;
  executor: IToolExecutorConstructor;
}

class ToolTypeRegistry {
  private entries = new Map<string, ToolTypeEntry>();

  register(pluginId: string, type: string, executor: IToolExecutorConstructor): void {
    this.entries.set(type, { pluginId, executor });
  }

  getExecutor(type: string): IToolExecutorConstructor | undefined {
    return this.entries.get(type)?.executor;
  }

  getAllExecutors(): Map<string, ToolTypeEntry> {
    return new Map(this.entries);
  }

  getAllToolTypes(): string[] {
    return Array.from(this.entries.keys());
  }

  getOwner(type: string): string | undefined {
    return this.entries.get(type)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [type, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(type);
      }
    }
  }
}

interface LLMProviderEntry {
  pluginId: string;
  formatter: BaseFormatter;
}

class LLMProviderRegistry {
  private entries = new Map<string, LLMProviderEntry>();

  register(pluginId: string, provider: string, formatter: BaseFormatter): void {
    this.entries.set(provider, { pluginId, formatter });
  }

  getFormatter(provider: string): BaseFormatter | undefined {
    return this.entries.get(provider)?.formatter;
  }

  getAllProviders(): string[] {
    return Array.from(this.entries.keys());
  }

  getOwner(provider: string): string | undefined {
    return this.entries.get(provider)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [provider, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(provider);
      }
    }
  }
}

interface FormatterEntry {
  pluginId: string;
  formatter: BaseFormatter;
}

class FormatterRegistry {
  private entries = new Map<string, FormatterEntry>();

  register(pluginId: string, name: string, formatter: BaseFormatter): void {
    this.entries.set(name, { pluginId, formatter });
  }

  getFormatter(name: string): BaseFormatter | undefined {
    return this.entries.get(name)?.formatter;
  }

  getOwner(name: string): string | undefined {
    return this.entries.get(name)?.pluginId;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [name, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(name);
      }
    }
  }
}

interface EventHandlerEntry {
  pluginId: string;
  handler: EventHandler;
  priority?: number;
}

class EventHandlerRegistry {
  private entries = new Map<string, EventHandlerEntry>();

  register(pluginId: string, eventType: string, handler: EventHandler, priority?: number): void {
    this.entries.set(eventType, { pluginId, handler, priority });
  }

  getAllHandlers(): Map<string, EventHandler> {
    const result = new Map<string, EventHandler>();
    for (const [eventType, entry] of this.entries) {
      result.set(eventType, entry.handler);
    }
    return result;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [eventType, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(eventType);
      }
    }
  }
}

interface HookHandlerEntry {
  pluginId: string;
  handler: HookHandler;
}

class HookHandlerRegistry {
  private entries = new Map<string, HookHandlerEntry>();

  register(pluginId: string, hookType: string, handler: HookHandler): void {
    this.entries.set(hookType, { pluginId, handler });
  }

  getAllHandlers(): Map<string, HookHandler> {
    const result = new Map<string, HookHandler>();
    for (const [hookType, entry] of this.entries) {
      result.set(hookType, entry.handler);
    }
    return result;
  }

  unregisterByPluginId(pluginId: string): void {
    for (const [hookType, entry] of this.entries) {
      if (entry.pluginId === pluginId) {
        this.entries.delete(hookType);
      }
    }
  }
}

interface MiddlewareItem {
  pluginId: string;
  mw: ExecutionMiddleware;
}

class MiddlewareRegistry {
  private entries = new Map<MiddlewarePhase, MiddlewareItem[]>();

  register(phase: MiddlewarePhase, mw: ExecutionMiddleware): void {
    if (!this.entries.has(phase)) {
      this.entries.set(phase, []);
    }
    this.entries.get(phase)!.push({ pluginId: '', mw });
    // Sort by priority (lower number = higher priority)
    this.entries.get(phase)!.sort((a, b) => (a.mw.priority ?? 0) - (b.mw.priority ?? 0));
  }

  getMiddleware(phase: MiddlewarePhase): ExecutionMiddleware[] {
    return this.entries.get(phase)?.map(item => item.mw) ?? [];
  }

  hasMiddleware(phase: MiddlewarePhase): boolean {
    const items = this.entries.get(phase);
    return items !== undefined && items.length > 0;
  }

  async runMiddleware(phase: MiddlewarePhase, context: Record<string, unknown>): Promise<void> {
    const items = this.entries.get(phase);
    if (!items || items.length === 0) return;

    let index = 0;
    const next = async (): Promise<void> => {
      if (index < items.length) {
        const item = items[index++]!;
        await item.mw.handler(context, next);
      }
    };
    await next();
  }

  unregisterByPluginId(pluginId: string): void {
    const phasesToDelete: MiddlewarePhase[] = [];
    for (const [phase, items] of this.entries) {
      const remaining = items.filter(item => item.pluginId !== pluginId);
      if (remaining.length === 0) {
        phasesToDelete.push(phase);
      } else {
        items.length = 0;
        items.push(...remaining);
      }
    }
    for (const phase of phasesToDelete) {
      this.entries.delete(phase);
    }
  }
}

// ============================================================
// Type Conversion Helpers
// ============================================================

/**
 * Convert a PluginLLMFormatter to a BaseFormatter.
 */
function toBaseFormatter(formatter: PluginLLMFormatter): BaseFormatter {
  return {
    format: async (request: PluginLLMRequest): Promise<PluginLLMResponse> => {
      return formatter.format(request);
    },
  } as unknown as BaseFormatter;
}

// ============================================================
// Contribution Manager
// ============================================================

/**
 * ContributionManager - Central manager for all plugin contributions.
 *
 * Implements ContributionRegistrar directly, so plugins interact with
 * this single instance through the registrar interface.
 */
export class ContributionManager implements ContributionRegistrar {
  private logger = createContextualLogger({ component: 'ContributionManager' });

  private nodeTypeRegistry = new NodeTypeRegistry();
  private toolTypeRegistry = new ToolTypeRegistry();
  private llmProviderRegistry = new LLMProviderRegistry();
  private formatterRegistry = new FormatterRegistry();
  private eventHandlerRegistry = new EventHandlerRegistry();
  private hookHandlerRegistry = new HookHandlerRegistry();
  private middlewareRegistry = new MiddlewareRegistry();

  private overridePolicy: OverridePolicy = OverridePolicy.FORBID;
  private currentPluginId: string = '';

  // ============================================================
  // Context Management
  // ============================================================

  /**
   * Set the plugin context for the current registration session.
   * Called by PluginEngine before invoking plugin.registerContributions().
   */
  setPluginContext(pluginId: string, overridePolicy?: OverridePolicy): void {
    this.currentPluginId = pluginId;
    if (overridePolicy !== undefined) {
      this.overridePolicy = overridePolicy;
    }
  }

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
  // ContributionRegistrar Implementation
  // ============================================================

  get nodeTypes(): NodeTypeRegistrar | undefined {
    return this.supportsType('node-type') ? this : undefined;
  }

  get toolTypes(): ToolTypeRegistrar | undefined {
    return this.supportsType('tool-type') ? this : undefined;
  }

  get llmProviders(): LLMProviderRegistrar | undefined {
    return this.supportsType('llm-provider') ? this : undefined;
  }

  get formatters(): FormatterRegistrar | undefined {
    return this.supportsType('formatter') ? this : undefined;
  }

  get eventHandlers(): EventHandlerRegistrar | undefined {
    return this.supportsType('event-handler') ? this : undefined;
  }

  get hookHandlers(): HookHandlerRegistrar | undefined {
    return this.supportsType('hook-handler') ? this : undefined;
  }

  get middleware(): MiddlewareRegistrar | undefined {
    return this.supportsType('middleware') ? this : undefined;
  }

  // ============================================================
  // NodeTypeRegistrar Implementation
  // ============================================================

  registerNodeType(type: string, handler: PluginNodeHandler): void {
    const nodeHandlerFn: NodeHandlerFn = async (_globalContext, _workflowExecutionEntity, _node, _context) => {
      const context = _context as Record<string, unknown> | undefined;
      const node = _node as unknown as Record<string, unknown> | undefined;
      const result = await handler.execute({
        nodeId: (node?.['id'] as string) ?? '',
        inputs: (context?.['inputs'] as Record<string, unknown>) ?? {},
        config: (node?.['config'] as Record<string, unknown>) ?? {},
      });
      return result.outputs;
    };
    this.registerNodeTypeInternal(this.currentPluginId, type, nodeHandlerFn);
  }

  // ============================================================
  // ToolTypeRegistrar Implementation
  // ============================================================

  registerToolType(type: string, executor: PluginToolExecutor): void {
    const executorConstructor = class implements IToolExecutor {
      async execute(
        _tool: Tool,
        parameters: Record<string, unknown>,
        _options?: ToolExecutionOptions,
        _executionId?: string,
        _context?: Record<string, unknown>,
      ): Promise<ToolExecutionResult> {
        const result = await executor.execute({ args: parameters ?? {} });
        return { result: result.result } as ToolExecutionResult;
      }
      getExecutorType(): string {
        return type;
      }
    };
    this.registerToolTypeInternal(this.currentPluginId, type, executorConstructor as unknown as IToolExecutorConstructor);
  }

  // ============================================================
  // LLMProviderRegistrar Implementation
  // ============================================================

  registerLLMProvider(provider: string, formatter: PluginLLMFormatter): void {
    this.registerLLMProviderInternal(this.currentPluginId, provider, toBaseFormatter(formatter));
  }

  // ============================================================
  // FormatterRegistrar Implementation
  // ============================================================

  registerFormatter(name: string, formatter: PluginLLMFormatter): void {
    this.registerFormatterInternal(this.currentPluginId, name, toBaseFormatter(formatter));
  }

  // ============================================================
  // EventHandlerRegistrar Implementation
  // ============================================================

  registerEventHandler(eventType: string, handler: PluginEventHandler): void {
    const eventHandler: EventHandler = (event: Record<string, unknown>) => {
      return handler.handle({ type: eventType, data: event });
    };
    this.registerEventHandlerInternal(this.currentPluginId, eventType, eventHandler);
  }

  // ============================================================
  // HookHandlerRegistrar Implementation
  // ============================================================

  registerHookHandler(hookType: string, handler: PluginHookHandler): void {
    const hookHandler: HookHandler = (context: Record<string, unknown>) => {
      return handler.handle(context);
    };
    this.registerHookHandlerInternal(this.currentPluginId, hookType, hookHandler);
  }

  // ============================================================
  // MiddlewareRegistrar Implementation
  // ============================================================

  registerMiddleware(_phase: string, middleware: PluginExecutionMiddleware): void {
    const phaseKey = middleware.phase as MiddlewarePhase;
    const executionMw: ExecutionMiddleware = {
      phase: phaseKey,
      handler: middleware.handler,
      priority: middleware.priority,
    };
    this.registerMiddlewareInternal(this.currentPluginId, phaseKey, executionMw);
  }

  // ============================================================
  // Internal Registration Methods
  // ============================================================

  private registerNodeTypeInternal(
    pluginId: string,
    nodeType: string,
    handler: NodeHandlerFn,
    template?: Record<string, unknown>,
  ): void {
    this.handleConflict('node-type', nodeType, pluginId, () => {
      this.nodeTypeRegistry.register(pluginId, nodeType, handler, template);
    });
  }

  private registerToolTypeInternal(pluginId: string, type: string, executor: IToolExecutorConstructor): void {
    this.handleConflict('tool-type', type, pluginId, () => {
      this.toolTypeRegistry.register(pluginId, type, executor);
    });
  }

  private registerLLMProviderInternal(pluginId: string, provider: string, formatter: BaseFormatter): void {
    this.handleConflict('llm-provider', provider, pluginId, () => {
      this.llmProviderRegistry.register(pluginId, provider, formatter);
    });
  }

  private registerFormatterInternal(pluginId: string, name: string, formatter: BaseFormatter): void {
    this.handleConflict('formatter', name, pluginId, () => {
      this.formatterRegistry.register(pluginId, name, formatter);
    });
  }

  private registerEventHandlerInternal(pluginId: string, eventType: string, handler: EventHandler, priority?: number): void {
    this.eventHandlerRegistry.register(pluginId, eventType, handler, priority);
  }

  private registerHookHandlerInternal(pluginId: string, hookType: string, handler: HookHandler): void {
    this.hookHandlerRegistry.register(pluginId, hookType, handler);
  }

  private registerMiddlewareInternal(_pluginId: string, phase: MiddlewarePhase, mw: ExecutionMiddleware): void {
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