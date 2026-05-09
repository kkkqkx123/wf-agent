/**
 * Container Manager - Multi-Instance DI Container Support
 * 
 * Manages multiple DI container instances for isolated SDK instances.
 * Each SDK instance gets its own container with independent storage adapters.
 * 
 * Design Principles:
 * - Each container is fully isolated with its own service bindings
 * - Containers can be created and destroyed independently
 * - Shared services (registries, executors) are bound per-container
 * - No global singleton state
 */

import { Container } from "@wf-agent/common-utils";
import type {
  CheckpointStorageAdapter,
  WorkflowStorageAdapter,
  TaskStorageAdapter,
  WorkflowExecutionStorageAdapter,
  AgentLoopCheckpointStorageAdapter,
} from "@wf-agent/storage";
import { configureContainerBindings } from "./container-config.js";

/**
 * Storage adapter configuration for a container instance
 */
export interface ContainerStorageConfig {
  checkpoint?: CheckpointStorageAdapter;
  workflow?: WorkflowStorageAdapter;
  task?: TaskStorageAdapter;
  workflowExecution?: WorkflowExecutionStorageAdapter;
  agentLoopCheckpoint?: AgentLoopCheckpointStorageAdapter;
}

/**
 * Container Manager - Manages multiple DI container instances
 */
export class ContainerManager {
  private static instance: ContainerManager | null = null;
  private containers: Map<string, Container> = new Map();

  private constructor() {}

  /**
   * Get the singleton ContainerManager instance
   */
  static getInstance(): ContainerManager {
    if (!ContainerManager.instance) {
      ContainerManager.instance = new ContainerManager();
    }
    return ContainerManager.instance;
  }

  /**
   * Create a new isolated container with storage adapters
   * 
   * @param containerId Unique identifier for this container
   * @param adapters Storage adapter configuration
   * @returns The configured container instance
   */
  createContainer(containerId: string, adapters: ContainerStorageConfig = {}): Container {
    if (this.containers.has(containerId)) {
      throw new Error(`Container with ID '${containerId}' already exists. Use destroyContainer() first.`);
    }

    const container = new Container();
    
    // Configure all service bindings for this container
    configureContainerBindings(container, adapters);
    
    this.containers.set(containerId, container);
    
    return container;
  }

  /**
   * Get an existing container by ID
   * 
   * @param containerId Container identifier
   * @returns The container instance
   * @throws Error if container not found
   */
  getContainer(containerId: string): Container {
    const container = this.containers.get(containerId);
    if (!container) {
      throw new Error(`Container with ID '${containerId}' not found. Create it first using createContainer().`);
    }
    return container;
  }

  /**
   * Check if a container exists
   * 
   * @param containerId Container identifier
   * @returns true if container exists
   */
  hasContainer(containerId: string): boolean {
    return this.containers.has(containerId);
  }

  /**
   * Destroy a container and release its resources
   * 
   * @param containerId Container identifier
   * @returns true if container was destroyed
   */
  async destroyContainer(containerId: string): Promise<boolean> {
    const container = this.containers.get(containerId);
    if (!container) {
      return false;
    }

    // Clear all caches
    container.clearAllCaches();
    
    // Remove from registry
    this.containers.delete(containerId);
    
    return true;
  }

  /**
   * Get all container IDs
   * 
   * @returns Array of container IDs
   */
  getAllContainerIds(): string[] {
    return Array.from(this.containers.keys());
  }

  /**
   * Get the number of active containers
   * 
   * @returns Number of containers
   */
  getContainerCount(): number {
    return this.containers.size;
  }

  /**
   * Destroy all containers
   */
  async destroyAllContainers(): Promise<void> {
    const containerIds = Array.from(this.containers.keys());
    for (const containerId of containerIds) {
      await this.destroyContainer(containerId);
    }
  }

  /**
   * Reset the ContainerManager (for testing)
   */
  async reset(): Promise<void> {
    await this.destroyAllContainers();
    ContainerManager.instance = null;
  }
}

/**
 * Convenience function to create a container without using the manager directly
 * Useful for simple cases where you don't need to track multiple containers
 * 
 * @param adapters Storage adapter configuration
 * @returns A new container instance with a generated ID
 */
export function createIsolatedContainer(adapters: ContainerStorageConfig = {}): { container: Container; containerId: string } {
  const manager = ContainerManager.getInstance();
  const containerId = `container-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  const container = manager.createContainer(containerId, adapters);
  return { container, containerId };
}
