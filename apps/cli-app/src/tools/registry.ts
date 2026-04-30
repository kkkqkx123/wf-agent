/**
 * Tool Registry Center
 * Reuses FunctionRegistry and StatefulExecutor from the tool-executors package
 */

import type { Tool } from "@wf-agent/types";
import {
  FunctionRegistry,
  StatefulExecutor,
  toSdkTool,
  type ToolDefinitionLike,
} from "@wf-agent/tool-executors";
import { createPredefinedTools, type PredefinedToolsOptions } from "@wf-agent/sdk";
import type { ToolRegistryConfig } from "./types.js";
import { getOutput } from "../utils/output.js";

const output = getOutput();

/**
 * Tool Registry Center
 * Reuses tool-executors capabilities
 */
export class ToolRegistry {
  private config: ToolRegistryConfig;
  private tools: Map<string, ToolDefinitionLike> = new Map();

  /** Reuse FunctionRegistry to manage stateless utility functions. */
  private functionRegistry: FunctionRegistry;

  /** Reusing StatefulExecutor to manage stateful tool instances */
  private statefulExecutor: StatefulExecutor;

  constructor(config: ToolRegistryConfig = {}) {
    this.config = {
      workspaceDir: config.workspaceDir || process.cwd(),
      memoryFile: config.memoryFile || "./workspace/.agent_memory.json",
    };

    // Initialize the tool-executors component
    this.functionRegistry = new FunctionRegistry({
      enableVersionControl: true,
      enableCallStatistics: true,
      maxFunctions: 100,
    });

    this.statefulExecutor = new StatefulExecutor({
      enableInstanceCache: true,
      maxCachedInstances: 100,
      instanceExpirationTime: 3600000, // 1 hour
      autoCleanupExpiredInstances: true,
      cleanupInterval: 300000, // 5 minutes
    });
  }

  /**
   * Register all built-in tools
   * Using SDK predefined tools
   */
  async registerAll(): Promise<void> {
    // Build SDK predefined tool configuration
    const predefinedOptions: PredefinedToolsOptions = {
      config: {
        readFile: { workspaceDir: this.config.workspaceDir! },
        writeFile: { workspaceDir: this.config.workspaceDir! },
        editFile: { workspaceDir: this.config.workspaceDir! },
        sessionNote: {
          workspaceDir: this.config.workspaceDir!,
          memoryFile: this.config.memoryFile!,
        },
      },
    };

    // Get predefined tools from the SDK
    const predefinedTools = createPredefinedTools(predefinedOptions);

    // Register with the local registry.
    for (const tool of predefinedTools) {
      this.register(tool);
    }

    output.infoLog(`Registered ${predefinedTools.length} predefined tools from SDK`);
  }

  /**
   * Register single tool
   */
  register(tool: ToolDefinitionLike): void {
    if (this.tools.has(tool.id)) {
      output.warnLog(`Tool '${tool.id}' already registered, overwriting...`);
    }
    this.tools.set(tool.id, tool);

    // Register with the corresponding executor as well.
    if (tool.type === "STATELESS" && tool.execute) {
      // Reusing the FunctionRegistry to register stateless tools
      this.functionRegistry.register(tool.id, tool.execute, tool.version, tool.description);
    }
    // Stateful tools are managed by the StatefulExecutor while in execution.
  }

  /**
   * Get tool definition
   */
  get(toolId: string): ToolDefinitionLike | undefined {
    return this.tools.get(toolId);
  }

  /**
   * Get all tool definitions
   */
  getAll(): ToolDefinitionLike[] {
    return Array.from(this.tools.values());
  }

  /**
   * Get all SDK format tools
   * Using toSdkTool function from tool-executors
   */
  getAllSdkTools(): Tool[] {
    return this.getAll().map(t => toSdkTool(t));
  }

  /**
   * Check if tool exists
   */
  has(toolId: string): boolean {
    return this.tools.has(toolId);
  }

  /**
   * Unregister tool
   */
  unregister(toolId: string): boolean {
    // Unregister from FunctionRegistry
    this.functionRegistry.unregister(toolId);
    return this.tools.delete(toolId);
  }

  /**
   * Clear all tools
   */
  clear(): void {
    this.functionRegistry.clear();
    this.tools.clear();
  }

  /**
   * Get tool count
   */
  get size(): number {
    return this.tools.size;
  }

  /**
   * Get FunctionRegistry (for external use)
   */
  getFunctionRegistry(): FunctionRegistry {
    return this.functionRegistry;
  }

  /**
   * Get StatefulExecutor (for external use)
   */
  getStatefulExecutor(): StatefulExecutor {
    return this.statefulExecutor;
  }

  /**
   * Get function statistics (reusing FunctionRegistry capability)
   */
  getFunctionStats(toolId: string) {
    return this.functionRegistry.getFunctionStats(toolId);
  }

  /**
   * Get all function names (reusing FunctionRegistry capability)
   */
  getFunctionNames(): string[] {
    return this.functionRegistry.getFunctionNames();
  }

  /**
   * Cleanup stateful tool instances for specified workflow execution (reusing StatefulExecutor capability)
   */
  cleanupWorkflowExecution(executionId: string): void {
    this.statefulExecutor.cleanupExecution(executionId);
  }

  /**
   * Cleanup resources
   */
  async cleanup(): Promise<void> {
    await this.statefulExecutor.cleanup();
    this.functionRegistry.clear();
    this.tools.clear();
  }
}

/**
 * Create tool registry instance
 */
export function createToolRegistry(config?: ToolRegistryConfig): ToolRegistry {
  return new ToolRegistry(config);
}
