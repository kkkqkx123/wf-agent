/**
 * MCP Executor Factory
 *
 * Creates and manages MCP executor instances with connection pooling support.
 */

import { McpServerExecutor } from "./mcp-server-executor.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "McpExecutorFactory" });

/**
 * Executor pool entry
 */
interface ExecutorPoolEntry {
  executor: McpServerExecutor;
  refCount: number;
  lastUsed: number;
}

/**
 * MCP Executor Factory
 * Manages creation and pooling of MCP server executors
 */
export class McpExecutorFactory {
  private static instance: McpExecutorFactory | null = null;
  private executors = new Map<string, ExecutorPoolEntry>();
  private cleanupInterval: ReturnType<typeof setInterval> | null = null;
  private cleanupIntervalMs = 60000; // 1 minute

  private constructor() {
    this.startCleanupTimer();
  }

  /**
   * Get factory singleton instance
   */
  static getInstance(): McpExecutorFactory {
    if (!McpExecutorFactory.instance) {
      McpExecutorFactory.instance = new McpExecutorFactory();
    }
    return McpExecutorFactory.instance;
  }

  /**
   * Get or create executor for server
   */
  getOrCreateExecutor(serverName: string): McpServerExecutor {
    const entry = this.executors.get(serverName);

    if (entry) {
      entry.refCount++;
      entry.lastUsed = Date.now();
      logger.debug("Reusing executor from pool", {
        serverName,
        refCount: entry.refCount,
      });
      return entry.executor;
    }

    const executor = new McpServerExecutor(serverName);
    this.executors.set(serverName, {
      executor,
      refCount: 1,
      lastUsed: Date.now(),
    });

    logger.debug("Created new executor", { serverName });
    return executor;
  }

  /**
   * Release executor reference
   */
  releaseExecutor(serverName: string): void {
    const entry = this.executors.get(serverName);

    if (!entry) {
      logger.warn("Releasing non-existent executor", { serverName });
      return;
    }

    entry.refCount--;
    logger.debug("Released executor reference", {
      serverName,
      refCount: entry.refCount,
    });

    if (entry.refCount <= 0) {
      this.executors.delete(serverName);
      logger.debug("Removed executor from pool", { serverName });
    }
  }

  /**
   * Get factory statistics
   */
  getStats(): {
    poolSize: number;
    executors: Array<{
      serverName: string;
      refCount: number;
      lastUsed: number;
    }>;
  } {
    return {
      poolSize: this.executors.size,
      executors: Array.from(this.executors.entries()).map(([serverName, entry]) => ({
        serverName,
        refCount: entry.refCount,
        lastUsed: entry.lastUsed,
      })),
    };
  }

  /**
   * Start cleanup timer to remove unused executors
   */
  private startCleanupTimer(): void {
    this.cleanupInterval = setInterval(() => {
      this.cleanup();
    }, this.cleanupIntervalMs);
  }

  /**
   * Clean up unused executors
   */
  private cleanup(): void {
    const now = Date.now();
    const maxIdleTime = 5 * 60 * 1000; // 5 minutes

    for (const [serverName, entry] of this.executors.entries()) {
      if (entry.refCount === 0 && now - entry.lastUsed > maxIdleTime) {
        this.executors.delete(serverName);
        logger.debug("Cleaned up idle executor", { serverName });
      }
    }
  }

  /**
   * Shutdown factory and cleanup all executors
   */
  async shutdown(): Promise<void> {
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
      this.cleanupInterval = null;
    }

    for (const [serverName, entry] of this.executors.entries()) {
      try {
        if (entry.executor.isConnected()) {
          await entry.executor.disconnect();
        }
      } catch (error) {
        logger.error("Error disconnecting executor during shutdown", {
          serverName,
          error,
        });
      }
    }

    this.executors.clear();
    logger.debug("Factory shutdown complete");
  }
}
