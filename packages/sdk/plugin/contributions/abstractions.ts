/**
 * Plugin Abstractions - Plugin-agnostic interfaces for the contribution system.
 *
 * These interfaces do NOT depend on SDK internal types (services/*, workflow/*).
 * They define the contract between plugins and the SDK contribution system.
 */

// ============================================================
// Core Plugin Handler Interfaces
// ============================================================

export interface PluginNodeHandler {
  type: string;
  execute(context: PluginExecutionContext): Promise<PluginNodeResult>;
}

export interface PluginToolExecutor {
  execute(context: PluginToolContext): Promise<PluginToolResult>;
}

export interface PluginToolInstance {
  name: string;
  description?: string;
  executor: PluginToolExecutor;
}

export interface PluginLLMFormatter {
  format(request: PluginLLMRequest): Promise<PluginLLMResponse>;
}

export interface PluginEventHandler {
  handle(event: PluginMessage): void | Promise<void>;
}

export interface PluginHookHandler {
  handle(context: Record<string, unknown>): Promise<void>;
}

export interface PluginExecutionMiddleware {
  phase: string;
  handler: (context: Record<string, unknown>, next: () => Promise<void>) => Promise<void>;
  priority: number;
}

// ============================================================
// Execution Context Types (minimal, stable)
// ============================================================

export interface PluginExecutionContext {
  nodeId: string;
  inputs: Record<string, unknown>;
  config: Record<string, unknown>;
}

export interface PluginNodeResult {
  outputs: Record<string, unknown>;
}

export interface PluginToolContext {
  args: Record<string, unknown>;
}

export interface PluginToolResult {
  result: unknown;
}

export interface PluginMessage {
  type: string;
  data: Record<string, unknown>;
}

export interface PluginLLMConfig {
  model: string;
  provider: string;
  temperature?: number;
  maxTokens?: number;
}

export interface PluginLLMRequest {
  messages: { role: string; content: string }[];
  config?: PluginLLMConfig;
}

export interface PluginLLMResponse {
  content: string;
  usage?: {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
  };
}