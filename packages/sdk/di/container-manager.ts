/**
 * Container Manager - Global DI Container Registry
 *
 * Manages multiple DI container instances for isolated SDK instances.
 * Each SDK instance gets its own container with independent service bindings.
 *
 * Design Principles:
 * - ContainerManager is a GLOBAL SINGLETON (process-level) acting as a registry
 * - Each container is fully isolated with its own service bindings
 * - Containers can be created and destroyed independently
 * - No shared business state between containers
 * - Similar to database connection pool managers or Express app registries
 *
 * Architecture:
 * ```
 * Process Level:
 *   ContainerManager (singleton) -> manages all containers
 *     ├─ Container 1 (SDK Instance 1)
 *     │   ├─ WorkflowRegistry (container singleton)
 *     │   ├─ ToolRegistry (container singleton)
 *     │   └─ ...
 *     ├─ Container 2 (SDK Instance 2)
 *     │   ├─ WorkflowRegistry (container singleton, independent)
 *     │   ├─ ToolRegistry (container singleton, independent)
 *     │   └─ ...
 * ```
 */

import { Container } from "@wf-agent/common-utils";
import { configureContainerBindings, type ContainerStorageConfig } from "./container-config.js";
import { generateId } from "@sdk/utils/id-utils.js";

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
   * Note: GlobalContext uses lazy loading, so it can be safely bound after container creation.
   * No two-phase initialization is needed.
   *
   * @param containerId Unique identifier for this container
   * @param adapters Storage adapter configuration
   * @returns The configured container instance
   */
  createContainer(containerId: string, adapters: ContainerStorageConfig = {}): Container {
    if (this.containers.has(containerId)) {
      throw new Error(
        `Container with ID '${containerId}' already exists. Use destroyContainer() first.`,
      );
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
      throw new Error(
        `Container with ID '${containerId}' not found. Create it first using createContainer().`,
      );
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
   * @throws Error if container not found
   */
  async destroyContainer(containerId: string): Promise<void> {
    const container = this.containers.get(containerId);
    if (!container) {
      throw new Error(`Container with ID '${containerId}' not found.`);
    }

    // Clear all caches
    container.clearAllCaches();

    // Remove from registry
    this.containers.delete(containerId);
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
 * Convenience function to create an isolated container
 * Useful for simple cases where you don't need to track multiple containers
 *
 * @param adapters Storage adapter configuration
 * @returns A new container instance with a generated ID
 */
export function createIsolatedContainer(adapters: ContainerStorageConfig = {}): {
  container: Container;
  containerId: string;
} {
  const manager = ContainerManager.getInstance();
  const containerId = `container-${generateId()}`;
  const container = manager.createContainer(containerId, adapters);
  return { container, containerId };
}
