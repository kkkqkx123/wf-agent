/**
 * Contribution Registrar - Sub-interfaces and implementations for the contribution system.
 *
 * Defines the sub-interfaces for each contribution type and the combined ContributionRegistrar.
 * The monolithic ContributionRegistrar has been split into focused sub-interfaces
 * following the Interface Segregation Principle.
 */

import type { HookHandler, EventHandler, IToolExecutorConstructor } from "./types.js";
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
import type { MiddlewarePhase } from "./middleware.types.js";
import type { BaseFormatter } from "../../services/llm/formatters/base.js";
import type { NodeHandlerFn } from "../../workflow/execution/handlers/node-handlers/index.js";
import { ContributionManager } from "./manager.js";
import { OverridePolicy } from "../config.js";

// ============================================================
// Sub-Interfaces (Interface Segregation)
// ============================================================

export interface NodeTypeRegistrar {
  registerNodeType(type: string, handler: PluginNodeHandler): void;
}

export interface ToolTypeRegistrar {
  registerToolType(type: string, executor: PluginToolExecutor): void;
}

export interface LLMProviderRegistrar {
  registerLLMProvider(provider: string, formatter: PluginLLMFormatter): void;
}

export interface FormatterRegistrar {
  registerFormatter(name: string, formatter: PluginLLMFormatter): void;
}

export interface EventHandlerRegistrar {
  registerEventHandler(eventType: string, handler: PluginEventHandler): void;
}

export interface HookHandlerRegistrar {
  registerHookHandler(hookType: string, handler: PluginHookHandler): void;
}

export interface MiddlewareRegistrar {
  registerMiddleware(phase: string, middleware: PluginExecutionMiddleware): void;
}

// ============================================================
// Combined Registrar Interface
// ============================================================

/**
 * ContributionRegistrar - Combined interface for plugins to register their contributions.
 * Each sub-registrar is optional and accessed via a property.
 *
 * Passed to the plugin's registerContributions() method.
 *
 * @deprecated Use the individual sub-interfaces directly. The combined interface
 *             is kept for backward compatibility with the Plugin.registerContributions() pattern.
 */
export interface ContributionRegistrar {
  nodeTypes?: NodeTypeRegistrar;
  toolTypes?: ToolTypeRegistrar;
  llmProviders?: LLMProviderRegistrar;
  formatters?: FormatterRegistrar;
  eventHandlers?: EventHandlerRegistrar;
  hookHandlers?: HookHandlerRegistrar;
  middleware?: MiddlewareRegistrar;
}

// ============================================================
// Shared Helper
// ============================================================

/**
 * Convert a PluginLLMFormatter to a BaseFormatter.
 * Used by both LLMProviderRegistrarImpl and FormatterRegistrarImpl.
 */
function toBaseFormatter(formatter: PluginLLMFormatter): BaseFormatter {
  return {
    format: async (request: PluginLLMRequest): Promise<PluginLLMResponse> => {
      return formatter.format(request);
    },
  } as unknown as BaseFormatter;
}

// ============================================================
// Sub-Interface Implementations
// ============================================================

export class NodeTypeRegistrarImpl implements NodeTypeRegistrar {
  constructor(
    private pluginId: string,
    private manager: ContributionManager,
  ) {}

  registerNodeType(type: string, handler: PluginNodeHandler): void {
    // Convert PluginNodeHandler to NodeHandlerFn
    const nodeHandlerFn: NodeHandlerFn = async (_globalContext, _workflowExecutionEntity, _node, _context) => {
      const node = _node as unknown as Record<string, unknown>;
      const ctx = _context as unknown as Record<string, unknown>;
      const getInputFn = ctx['getInput'] as (() => Record<string, unknown>) | undefined;
      const nodeId = (node['nodeId'] as string) || (node['id'] as string) || '';
      const config = (node['config'] as Record<string, unknown>) || {};
      const result = await handler.execute({
        nodeId,
        inputs: getInputFn?.() || {},
        config,
      });
      return result.outputs;
    };
    this.manager.registerNodeType(this.pluginId, type, nodeHandlerFn);
  }
}

export class ToolTypeRegistrarImpl implements ToolTypeRegistrar {
  constructor(
    private pluginId: string,
    private manager: ContributionManager,
  ) {}

  registerToolType(type: string, executor: PluginToolExecutor): void {
    // Convert PluginToolExecutor to IToolExecutor constructor
    const ExecutorClass = class {
      async execute(...args: unknown[]): Promise<unknown> {
        const result = await executor.execute({ args: args[0] as Record<string, unknown> || {} });
        return result.result;
      }
      getExecutorType(): string {
        return type;
      }
    };
    this.manager.registerToolType(this.pluginId, type, ExecutorClass as IToolExecutorConstructor);
  }
}

export class LLMProviderRegistrarImpl implements LLMProviderRegistrar {
  constructor(
    private pluginId: string,
    private manager: ContributionManager,
  ) {}

  registerLLMProvider(provider: string, formatter: PluginLLMFormatter): void {
    this.manager.registerLLMProvider(this.pluginId, provider, toBaseFormatter(formatter));
  }
}

export class FormatterRegistrarImpl implements FormatterRegistrar {
  constructor(
    private pluginId: string,
    private manager: ContributionManager,
  ) {}

  registerFormatter(name: string, formatter: PluginLLMFormatter): void {
    this.manager.registerFormatter(this.pluginId, name, toBaseFormatter(formatter));
  }
}

export class EventHandlerRegistrarImpl implements EventHandlerRegistrar {
  constructor(
    private pluginId: string,
    private manager: ContributionManager,
  ) {}

  registerEventHandler(eventType: string, handler: PluginEventHandler): void {
    const eventHandler: EventHandler = (event: Record<string, unknown>) => {
      return handler.handle({ type: eventType, data: event });
    };
    this.manager.registerEventHandler(this.pluginId, eventType, eventHandler);
  }
}

export class HookHandlerRegistrarImpl implements HookHandlerRegistrar {
  constructor(
    private pluginId: string,
    private manager: ContributionManager,
  ) {}

  registerHookHandler(hookType: string, handler: PluginHookHandler): void {
    const hookHandler: HookHandler = (context: Record<string, unknown>) => {
      return handler.handle(context);
    };
    this.manager.registerHookHandler(this.pluginId, hookType, hookHandler);
  }
}

export class MiddlewareRegistrarImpl implements MiddlewareRegistrar {
  constructor(
    private pluginId: string,
    private manager: ContributionManager,
  ) {}

  registerMiddleware(phase: string, middleware: PluginExecutionMiddleware): void {
    this.manager.registerMiddleware(
      this.pluginId,
      phase as MiddlewarePhase,
      {
        phase: middleware.phase as MiddlewarePhase,
        handler: middleware.handler,
        priority: middleware.priority,
      },
    );
  }
}

// ============================================================
// Combined Registrar Implementation
// ============================================================

/**
 * ContributionRegistrarImpl - Concrete implementation of the combined ContributionRegistrar.
 * Sub-registrar instances are cached on first access to avoid repeated allocations.
 */
export class ContributionRegistrarImpl implements ContributionRegistrar {
  private _nodeTypes?: NodeTypeRegistrar | null = null;
  private _toolTypes?: ToolTypeRegistrar | null = null;
  private _llmProviders?: LLMProviderRegistrar | null = null;
  private _formatters?: FormatterRegistrar | null = null;
  private _eventHandlers?: EventHandlerRegistrar | null = null;
  private _hookHandlers?: HookHandlerRegistrar | null = null;
  private _middleware?: MiddlewareRegistrar | null = null;

  constructor(
    private pluginId: string,
    private manager: ContributionManager,
    _overridePolicy?: OverridePolicy,
  ) {
    // _overridePolicy is reserved for future use
  }

  get nodeTypes(): NodeTypeRegistrar | undefined {
    if (this._nodeTypes === null) {
      this._nodeTypes = this.manager.supportsType('node-type')
        ? new NodeTypeRegistrarImpl(this.pluginId, this.manager)
        : undefined;
    }
    return this._nodeTypes;
  }

  get toolTypes(): ToolTypeRegistrar | undefined {
    if (this._toolTypes === null) {
      this._toolTypes = this.manager.supportsType('tool-type')
        ? new ToolTypeRegistrarImpl(this.pluginId, this.manager)
        : undefined;
    }
    return this._toolTypes;
  }

  get llmProviders(): LLMProviderRegistrar | undefined {
    if (this._llmProviders === null) {
      this._llmProviders = this.manager.supportsType('llm-provider')
        ? new LLMProviderRegistrarImpl(this.pluginId, this.manager)
        : undefined;
    }
    return this._llmProviders;
  }

  get formatters(): FormatterRegistrar | undefined {
    if (this._formatters === null) {
      this._formatters = this.manager.supportsType('formatter')
        ? new FormatterRegistrarImpl(this.pluginId, this.manager)
        : undefined;
    }
    return this._formatters;
  }

  get eventHandlers(): EventHandlerRegistrar | undefined {
    if (this._eventHandlers === null) {
      this._eventHandlers = this.manager.supportsType('event-handler')
        ? new EventHandlerRegistrarImpl(this.pluginId, this.manager)
        : undefined;
    }
    return this._eventHandlers;
  }

  get hookHandlers(): HookHandlerRegistrar | undefined {
    if (this._hookHandlers === null) {
      this._hookHandlers = this.manager.supportsType('hook-handler')
        ? new HookHandlerRegistrarImpl(this.pluginId, this.manager)
        : undefined;
    }
    return this._hookHandlers;
  }

  get middleware(): MiddlewareRegistrar | undefined {
    if (this._middleware === null) {
      this._middleware = this.manager.supportsType('middleware')
        ? new MiddlewareRegistrarImpl(this.pluginId, this.manager)
        : undefined;
    }
    return this._middleware;
  }
}