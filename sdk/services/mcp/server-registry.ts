/**
 * MCP Server Registry
 * Singleton manager for MCP server instances
 */

import type { McpManagerOptions, McpServerState } from "./types.js";
import { McpConnectionManager } from "./connection-manager.js";

/**
 * Registry state
 */
interface RegistryState {
  instance: McpConnectionManager | null;
  clientInfo: { name: string; version: string };
  options: McpManagerOptions;
  initializationPromise: Promise<McpConnectionManager> | null;
  refCount: number;
}

/**
 * Global registry state
 */
const registry: RegistryState = {
  instance: null,
  clientInfo: { name: "wf-agent-sdk", version: "1.0.0" },
  options: {},
  initializationPromise: null,
  refCount: 0,
};

/**
 * MCP Server Registry
 * Provides singleton access to MCP connection manager
 */
export class McpServerRegistry {
  /**
   * Set client info for MCP connections
   */
  static setClientInfo(info: { name: string; version: string }): void {
    registry.clientInfo = info;
  }

  /**
   * Set default options
   */
  static setOptions(options: McpManagerOptions): void {
    registry.options = options;
  }

  /**
   * Get the singleton McpConnectionManager instance
   * Workflow-execution-safe implementation using promise-based lock
   */
  static async getInstance(): Promise<McpConnectionManager> {
    // Increment reference count
    registry.refCount++;

    // If we already have an instance, return it
    if (registry.instance) {
      return registry.instance;
    }

    // If initialization is in progress, wait for it
    if (registry.initializationPromise) {
      return registry.initializationPromise;
    }

    // Create a new initialization promise
    registry.initializationPromise = (async () => {
      try {
        // Double-check instance in case it was created while we were waiting
        if (!registry.instance) {
          registry.instance = new McpConnectionManager(registry.clientInfo, registry.options);
        }
        return registry.instance;
      } finally {
        // Clear the initialization promise after completion or error
        registry.initializationPromise = null;
      }
    })();

    return registry.initializationPromise;
  }

  /**
   * Release a reference to the registry
   * If reference count reaches zero, disposes the instance
   */
  static async release(): Promise<void> {
    registry.refCount--;

    if (registry.refCount <= 0 && registry.instance) {
      await registry.instance.disconnectAll();
      registry.instance = null;
      registry.refCount = 0;
    }
  }

  /**
   * Check if an instance exists
   */
  static hasInstance(): boolean {
    return registry.instance !== null;
  }

  /**
   * Get current reference count
   */
  static getRefCount(): number {
    return registry.refCount;
  }

  /**
   * Force cleanup of the singleton instance
   */
  static async cleanup(): Promise<void> {
    if (registry.instance) {
      await registry.instance.disconnectAll();
      registry.instance = null;
    }
    registry.initializationPromise = null;
    registry.refCount = 0;
  }

  /**
   * Reset the registry to initial state
   * Useful for testing
   */
  static reset(): void {
    registry.instance = null;
    registry.initializationPromise = null;
    registry.refCount = 0;
  }
}

/**
 * Convenience function to get MCP manager
 */
export async function getMcpManager(): Promise<McpConnectionManager> {
  return McpServerRegistry.getInstance();
}

/**
 * Convenience function to release MCP manager
 */
export async function releaseMcpManager(): Promise<void> {
  return McpServerRegistry.release();
}
