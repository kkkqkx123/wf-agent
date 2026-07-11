/**
 * Server Dependency Container
 *
 * Centralized dependency management for the Server application.
 * Provides a single access point for all services and adapters.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";
import type { CLIConfig } from "../config/index.js";

/**
 * Server Dependency Container
 * Manages all major services and their dependencies
 */
export class ServerDependencyContainer {
  private sdk: SDKInstance;
  private config: CLIConfig;
  private adapters: Map<string, unknown> = new Map();
  private services: Map<string, unknown> = new Map();

  constructor(sdk: SDKInstance, config: CLIConfig) {
    this.sdk = sdk;
    this.config = config;
  }

  /**
   * Register an adapter
   */
  registerAdapter(name: string, adapter: unknown): void {
    this.adapters.set(name, adapter);
  }

  /**
   * Get an adapter
   */
  getAdapter<T>(name: string): T {
    const adapter = this.adapters.get(name);
    if (!adapter) {
      throw new Error(`Adapter "${name}" not found in container`);
    }
    return adapter as T;
  }

  /**
   * Register a service
   */
  registerService(name: string, service: unknown): void {
    this.services.set(name, service);
  }

  /**
   * Get a service
   */
  getService<T>(name: string): T {
    const service = this.services.get(name);
    if (!service) {
      throw new Error(`Service "${name}" not found in container`);
    }
    return service as T;
  }

  /**
   * Get the SDK instance
   */
  getSDK(): SDKInstance {
    return this.sdk;
  }

  /**
   * Get the configuration
   */
  getConfig(): CLIConfig {
    return this.config;
  }

  /**
   * Get all registered adapter names
   */
  getAdapterNames(): string[] {
    return Array.from(this.adapters.keys());
  }

  /**
   * Get all registered service names
   */
  getServiceNames(): string[] {
    return Array.from(this.services.keys());
  }

  /**
   * Cleanup all resources
   */
  async cleanup(): Promise<void> {
    // Services cleanup can be added here if needed
    // For now, SDK cleanup is handled separately
  }
}

/**
 * Global container instance
 * Used as singleton pattern for backward compatibility
 */
let globalContainer: ServerDependencyContainer | null = null;

/**
 * Initialize the global container
 */
export function initializeContainer(
  sdk: SDKInstance,
  config: CLIConfig
): ServerDependencyContainer {
  globalContainer = new ServerDependencyContainer(sdk, config);
  return globalContainer;
}

/**
 * Get the global container instance
 */
export function getContainer(): ServerDependencyContainer {
  if (!globalContainer) {
    throw new Error(
      "Container not initialized. Call initializeContainer() first."
    );
  }
  return globalContainer;
}

/**
 * Clear the global container (for testing)
 */
export function clearContainer(): void {
  globalContainer = null;
}
