/**
 * Stateful Tool Executor
 * Executes stateful tools provided by the application layer, directly manages tool instances, and supports thread isolation.
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
  // Manage instances by thread ID and tool name: Map<threadId, Map<toolName, { instance, createdAt }>>
  private threadInstances: Map<
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
   * @param threadId Thread ID (required, for thread isolation)
   * @returns Execution result
   */
  protected async doExecute(
    tool: Tool,
    parameters: Record<string, unknown>,
    threadId?: string,
  ): Promise<unknown> {
    if (!threadId) {
      throw new ToolError(
        `ThreadId is required for stateful tool '${tool.name}'`,
        tool.id,
        "STATEFUL",
        { toolName: tool.name, threadIdRequired: true },
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
      // Obtain or create a tool instance (thread-isolated).
      const instance = this.getOrCreateInstance(threadId!, tool.id, toolConfig.factory);

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
          threadId,
        });
      }

      // Returns the result, with ToolOutput.content as the result.
      return {
        result: output.content,
        threadId,
      };
    } catch (error) {
      if (error instanceof ToolError) {
        throw error;
      }
      throw new ToolError(
        `Stateful tool execution failed: ${error instanceof Error ? error.message : "Unknown error"}`,
        tool.id,
        "STATEFUL",
        { parameters, threadId },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Get or create a tool instance (thread-isolated)
   * @param threadId: Thread ID
   * @param toolName: Tool name
   * @param factory: Factory function
   * @returns: Tool instance
   */
  private getOrCreateInstance(
    threadId: string,
    toolName: string,
    factory: StatefulToolFactory,
  ): StatefulToolInstance {
    // Get or create an instance map for that thread.
    if (!this.threadInstances.has(threadId)) {
      this.threadInstances.set(threadId, new Map());
    }

    const threadMap = this.threadInstances.get(threadId)!;

    // If an instance already exists, return it directly.
    if (threadMap.has(toolName)) {
      return threadMap.get(toolName)!.instance;
    }

    // Create a new instance
    const instance = factory.create();
    threadMap.set(toolName, {
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
    for (const [threadId, threadMap] of this.threadInstances.entries()) {
      for (const [toolName, { createdAt }] of threadMap.entries()) {
        if (currentTime - createdAt > this.config.instanceExpirationTime!) {
          threadMap.delete(toolName);
        }
      }
      // If the thread no longer has any instances, delete the thread mapping.
      if (threadMap.size === 0) {
        this.threadInstances.delete(threadId);
      }
    }
  }

  /**
   * Clean up all instances of the specified thread.
   * @param threadId: Thread ID
   */
  cleanupThread(threadId: string): void {
    const threadMap = this.threadInstances.get(threadId);
    if (threadMap) {
      for (const [_toolName, { instance }] of threadMap.entries()) {
        if (typeof instance.destroy === "function") {
          instance.destroy();
        }
      }
      this.threadInstances.delete(threadId);
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
    for (const [, threadMap] of this.threadInstances.entries()) {
      for (const [_toolName, { instance }] of threadMap.entries()) {
        if (typeof instance.destroy === "function") {
          await instance.destroy();
        }
      }
    }
    this.threadInstances.clear();
  }

  /**
   * Get the executor type
   */
  getExecutorType(): string {
    return "STATEFUL";
  }
}
