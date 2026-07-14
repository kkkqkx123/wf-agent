/**
 * Tool Registry Center
 * Reuses StatelessExecutor and StatefulExecutor from the tool-executors package
 */

import type { Tool } from "@wf-agent/types";
import {
  StatelessExecutor,
  StatefulExecutor,
  toSdkTool,
  type ToolDefinitionLike,
} from "@wf-agent/sdk/services";
import { createPredefinedTools, closeStorage, cleanupSessionNotes, type PredefinedToolsOptions, type SkillHandlerConfig } from "@wf-agent/sdk/resources";
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

  /** Manage stateless tools execution */
  private statelessExecutor: StatelessExecutor;

  /** Manage stateful tool instances */
  private statefulExecutor: StatefulExecutor;

  constructor(config: ToolRegistryConfig = {}) {
    this.config = {
      workspaceDir: config.workspaceDir || process.cwd(),
      dbPath: config.dbPath || ".agent_memory.db",
    };

    // Initialize the tool-executors components
    this.statelessExecutor = new StatelessExecutor({
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
  async registerAll(skillConfig?: SkillHandlerConfig): Promise<void> {
    // Build SDK predefined tool configuration
    const predefinedOptions: PredefinedToolsOptions = {
      config: {
        readFile: { workspaceDir: this.config.workspaceDir! },
        writeFile: { workspaceDir: this.config.workspaceDir! },
        editFile: { workspaceDir: this.config.workspaceDir! },
        sessionNote: {
          workspaceDir: this.config.workspaceDir!,
          dbPath: this.config.dbPath!,
        },
        skill: skillConfig,
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
    return this.tools.delete(toolId);
  }

  /**
   * Clear all tools
   */
  clear(): void {
    this.statelessExecutor = new StatelessExecutor({
      enableVersionControl: true,
      enableCallStatistics: true,
      maxFunctions: 100,
    });
    this.tools.clear();
  }

  /**
   * Get tool count
   */
  get size(): number {
    return this.tools.size;
  }

  /**
   * Get StatelessExecutor (for external use)
   */
  getStatelessExecutor(): StatelessExecutor {
    return this.statelessExecutor;
  }

  /**
   * Get StatefulExecutor (for external use)
   */
  getStatefulExecutor(): StatefulExecutor {
    return this.statefulExecutor;
  }

  /**
   * Cleanup stateful tool instances for specified workflow execution (reusing StatefulExecutor capability)
   */
  cleanupWorkflowExecution(executionId: string): void {
    this.statefulExecutor.cleanupExecution(executionId);
    cleanupSessionNotes(executionId);
  }

  /**
   * Cleanup resources
   */
  async cleanup(): Promise<void> {
    await this.statefulExecutor.cleanup();
    this.tools.clear();
    closeStorage();
  }
}

/**
 * Create tool registry instance
 */
export function createToolRegistry(config?: ToolRegistryConfig): ToolRegistry {
  return new ToolRegistry(config);
}
