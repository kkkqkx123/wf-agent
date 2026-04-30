/**
 * Stateful Tool Executor
 * Executes stateful tools provided by the application layer, directly manages tool instances, and supports execution isolation.
 */

import { now } from "@wf-agent/common-utils";
import type { Tool, ToolOutput } from "@wf-agent/types";
import type {
  StatefulToolConfig,
  StatefulToolFactory,
  StatefulToolInstance,
} from "@wf-agent/types";
import { ToolError } from "@wf-agent/types";
import { BaseExecutor } from "../core/base/BaseExecutor.js";
import type { StatefulExecutorConfig } from "./types.js";

/**
 * Stateful tool executor
 */
export class StatefulExecutor extends BaseExecutor {
  private config: StatefulExecutorConfig;
  // Manage instances by execution ID and tool name: Map<executionId, Map<toolName, { instance, createdAt }>>
  private executionInstances: Map<
    string,
    Map<string, { instance: StatefulToolInstance; createdAt: number }>
  > = new Map();
  private cleanupTimer?: NodeJS.Timeout;

  constructor(config: StatefulExecutorConfig = {}) {
    super();
    this.config = {
      enableInstanceCache: config.enableInstanceCache ?? true,
      maxCachedInstances: config.maxCachedInstances ?? 100,
      instanceExpirationTime: config.instanceExpirationTime ?? 3600000, // 1 hour
      autoCleanupExpiredInstances: config.autoCleanupExpiredInstances ?? true,
      cleanupInterval: config.cleanupInterval ?? 300000, // 5 minutes
    };

    // Start automatic cleaning
    if (this.config.autoCleanupExpiredInstances) {
      this.startCleanupTimer();
    }
  }

  /**
   * Specific implementation of executing a stateful tool
   * @param tool Tool definition
   * @param parameters Tool parameters
   * @param executionId Execution ID (required, for execution isolation)
   * @returns Execution result
   */
  protected async doExecute(
    tool: Tool,
    parameters: Record<string, unknown>,
    executionId?: string,
  ): Promise<unknown> {
    if (!executionId) {
      throw new ToolError(
        `ExecutionId is required for stateful tool '${tool.name}'`,
        tool.id,
        "STATEFUL",
        { toolName: tool.name, executionIdRequired: true },
      );
    }

    // Get the factory function
    const toolConfig = tool.config as StatefulToolConfig;
    if (!toolConfig || !toolConfig.factory) {
      throw new ToolError(
        `Tool '${tool.name}' does not have a factory function`,
        tool.id,
        "STATEFUL",
        { toolName: tool.name, hasConfig: !!toolConfig, hasFactory: !!toolConfig?.factory },
      );
    }

    if (typeof toolConfig.factory.create !== "function") {
      throw new ToolError(`Factory for tool '${tool.id}' is not a function`, tool.id, "STATEFUL", {
        factoryCreateType: typeof toolConfig.factory.create,
      });
    }

    try {
      // Obtain or create a tool instance (execution-isolated).
      const instance = this.getOrCreateInstance(executionId!, tool.id, toolConfig.factory);

      // Call the execute method of the instance.
      if (typeof instance.execute !== "function") {
        throw new ToolError(
          `Tool instance for '${tool.id}' does not have an execute method`,
          tool.id,
          "STATEFUL",
          { instanceType: typeof instance, hasMethods: Object.keys(instance) },
        );
      }

      const output = (await instance.execute(parameters)) as ToolOutput;

      // If the tool execution fails, an error is thrown.
      if (!output.success) {
        throw new ToolError(output.error || "Tool execution failed", tool.id, "STATEFUL", {
          parameters,
          executionId,
        });
      }

      // Returns the result, with ToolOutput.content as the result.
      return {
        result: output.content,
        executionId,
      };
    } catch (error) {
      if (error instanceof ToolError) {
        throw error;
      }
      throw new ToolError(
        `Stateful tool execution failed: ${error instanceof Error ? error.message : "Unknown error"}`,
        tool.id,
        "STATEFUL",
        { parameters, executionId },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Get or create a tool instance (execution-isolated)
   * @param executionId: Execution ID
   * @param toolName: Tool name
   * @param factory: Factory function
   * @returns: Tool instance
   */
  private getOrCreateInstance(
    executionId: string,
    toolName: string,
    factory: StatefulToolFactory,
  ): StatefulToolInstance {
    // Get or create an instance map for that execution.
    if (!this.executionInstances.has(executionId)) {
      this.executionInstances.set(executionId, new Map());
    }

    const executionMap = this.executionInstances.get(executionId)!;

    // If an instance already exists, return it directly.
    if (executionMap.has(toolName)) {
      return executionMap.get(toolName)!.instance;
    }

    // Create a new instance
    const instance = factory.create();
    executionMap.set(toolName, {
      instance,
      createdAt: now(),
    });

    return instance;
  }

  /**
   * Start the cleanup timer
   */
  private startCleanupTimer(): void {
    this.cleanupTimer = setInterval(() => {
      this.cleanupExpiredInstances();
    }, this.config.cleanupInterval);
  }

  /**
   * Clean up expired instances.
   */
  private cleanupExpiredInstances(): void {
    const currentTime = now();
    for (const [executionId, executionMap] of this.executionInstances.entries()) {
      for (const [toolName, { createdAt }] of executionMap.entries()) {
        if (currentTime - createdAt > this.config.instanceExpirationTime!) {
          executionMap.delete(toolName);
        }
      }
      // If the execution no longer has any instances, delete the execution mapping.
      if (executionMap.size === 0) {
        this.executionInstances.delete(executionId);
      }
    }
  }

  /**
   * Clean up all instances of the specified execution.
   * @param executionId: Execution ID
   */
  cleanupExecution(executionId: string): void {
    const executionMap = this.executionInstances.get(executionId);
    if (executionMap) {
      for (const [_toolName, { instance }] of executionMap.entries()) {
        if (typeof instance.destroy === "function") {
          instance.destroy();
        }
      }
      this.executionInstances.delete(executionId);
    }
  }

  /**
   * Clean up resources
   */
  async cleanup(): Promise<void> {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = undefined;
    }
    // Clean up all instances.
    for (const [, executionMap] of this.executionInstances.entries()) {
      for (const [_toolName, { instance }] of executionMap.entries()) {
        if (typeof instance.destroy === "function") {
          await instance.destroy();
        }
      }
    }
    this.executionInstances.clear();
  }

  /**
   * Get the executor type
   */
  getExecutorType(): string {
    return "STATEFUL";
  }
}
