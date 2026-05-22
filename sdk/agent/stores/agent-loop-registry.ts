/**
 * AgentLoopRegistry - Agent Loop Registry
 *
 * Manages all active AgentLoopEntity instances.
 * Refer to the WorkflowExecutionRegistry design pattern.
 */

import type { ID } from "@wf-agent/types";
import type { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import { AgentLoopStatus } from "@wf-agent/types";
import type { AgentLoopStorageAdapter } from "@wf-agent/storage";
import type { AgentEntityMetadata } from "@wf-agent/types";
import type { IAgentExecutionRegistry, AgentExecutionFilter } from "./agent-execution-registry.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";

const logger = createContextualLogger({ component: "AgentLoopRegistry" });

/**
 * AgentLoopRegistry - Agent Loop Registry
 *
 * Core Responsibilities:
 * - Manage active AgentLoopEntity instances.
 * - Provide registration, query and deletion of instances
 *
 * Design Principles:
 * - Singleton model (managed via DI container)
 * - Workflow-execution-safe (Map operations)
 * - Support for cleaning up expired instances
 */
export class AgentLoopRegistry implements IAgentExecutionRegistry {
  /** Instance Storage */
  private entities: Map<ID, AgentLoopEntity> = new Map();
  private storageAdapter?: AgentLoopStorageAdapter;

  /**
   * Constructor
   * @param options Configuration options
   */
  constructor(options?: { storageAdapter?: AgentLoopStorageAdapter }) {
    this.storageAdapter = options?.storageAdapter;
  }

  /**
   * Register AgentLoopEntity
   * @param entity The Agent Loop entity
   */
  register(entity: AgentLoopEntity): void {
    this.entities.set(entity.id, entity);
    
    // Persist to storage (async, non-blocking)
    this.persistToStorage(entity).catch(error => {
      logger.error("Failed to persist agent loop to storage during register", {
        agentLoopId: entity.id,
        error: getErrorMessage(error),
      });
    });
  }

  /**
   * Logout AgentLoopEntity
   * @param id Instance ID
   * @returns Whether the logout was successful
   */
  unregister(id: ID): boolean {
    const result = this.entities.delete(id);
    
    if (result) {
      // Remove from storage (async, non-blocking)
      this.removeFromStorage(id).catch(error => {
        logger.error("Failed to remove agent loop from storage during unregister", {
          agentLoopId: id,
          error: getErrorMessage(error),
        });
      });
    }
    
    return result;
  }

  /**
   * Get AgentLoopEntity
   * Tries to get from memory first, then attempts to load from storage if not found.
   * @param id instance ID
   * @returns Agent Loop entity, or undefined if it doesn't exist.
   */
  async get(id: ID): Promise<AgentLoopEntity | undefined> {
    // Try memory first
    const cached = this.entities.get(id);
    if (cached) {
      return cached;
    }

    // If not in memory and storage adapter is available, try loading from storage
    if (this.storageAdapter) {
      logger.debug("Agent loop not in memory, attempting to load from storage", { agentLoopId: id });
      // Note: Full restoration requires checkpoint mechanism via AgentLoopFactory.fromCheckpoint()
      // This method only provides basic data loading. For complete entity restoration,
      // use the checkpoint restore API instead.
      const loadedData = await this._loadFromStorage(id);
      if (loadedData) {
        logger.warn(
          "Loaded raw data from storage but cannot reconstruct full entity without checkpoint. " +
            "Use AgentLoopFactory.fromCheckpoint() for complete restoration.",
          { agentLoopId: id }
        );
      }
    }

    return undefined;
  }

  /**
   * Checking if an instance exists
   * @param id instance ID
   */
  has(id: ID): boolean {
    return this.entities.has(id);
  }

  /**
   * Get all active instances
   */
  getAll(): AgentLoopEntity[] {
    return Array.from(this.entities.values());
  }

  /**
   * Get all instance IDs
   */
  getAllIds(): ID[] {
    return Array.from(this.entities.keys());
  }

  /**
   * Get the number of instances
   */
  size(): number {
    return this.entities.size;
  }

  /**
   * Getting Examples by Status
   * @param status Execution status
   */
  getByStatus(status: AgentLoopStatus): AgentLoopEntity[] {
    return this.getAll().filter(entity => entity.getStatus() === status);
  }

  /**
   * Getting a running instance
   */
  getRunning(): AgentLoopEntity[] {
    return this.getByStatus(AgentLoopStatus.RUNNING);
  }

  /**
   * Getting a Suspended Instance
   */
  getPaused(): AgentLoopEntity[] {
    return this.getByStatus(AgentLoopStatus.PAUSED);
  }

  /**
   * Getting Completed Instances
   */
  getCompleted(): AgentLoopEntity[] {
    return this.getByStatus(AgentLoopStatus.COMPLETED);
  }

  /**
   * Getting failed instances
   */
  getFailed(): AgentLoopEntity[] {
    return this.getByStatus(AgentLoopStatus.FAILED);
  }

  /**
   * Query agent loop entities with optional filter
   *
   * Supports filtering by:
   * - status: Filter by execution status
   * - parentWorkflowId: Filter by parent workflow execution ID
   *
   * @param filter Optional filter criteria
   * @returns Filtered array of AgentLoopEntity instances
   */
  query(filter?: AgentExecutionFilter): AgentLoopEntity[] {
    let results = this.getAll();

    if (filter?.status) {
      results = results.filter(entity => entity.getStatus() === filter.status);
    }

    if (filter?.parentWorkflowId) {
      results = results.filter(entity => {
        const parent = entity.getParentContext();
        return parent?.parentType === 'WORKFLOW' && parent.parentId === filter.parentWorkflowId;
      });
    }

    return results;
  }

  /**
   * Cleaning up completed instances
   * @returns Number of instances cleaned up
   */
  cleanupCompleted(): number {
    const completedIds = this.getCompleted().map(e => e.id);
    for (const id of completedIds) {
      this.unregister(id);
    }
    return completedIds.length;
  }

  /**
   * Clear all instances
   */
  clear(): void {
    this.entities.clear();
  }

  /**
   * Cleaning up resources
   * Calls the cleanup method of each entity before cleaning it up
   */
  cleanup(): void {
    for (const entity of this.entities.values()) {
      if (typeof entity.cleanup === "function") {
        entity.cleanup();
      }
    }
    this.entities.clear();
  }

  // ============================================================
  // Storage Persistence Methods
  // ============================================================

  /**
   * Persist agent loop to storage (if adapter is available)
   * @param entity Agent loop entity to persist
   */
  private async persistToStorage(entity: AgentLoopEntity): Promise<void> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping agent loop persistence");
      return;
    }

    try {
      // Serialize agent loop state to bytes
      const encoder = new TextEncoder();
      const agentData = {
        id: entity.id,
        status: entity.getStatus(),
        currentIteration: entity.state.currentIteration,
        toolCallCount: entity.state.toolCallCount,
        startTime: entity.state.startTime,
        endTime: entity.state.endTime,
        maxIterations: entity.config?.maxIterations,
      };
      const data = encoder.encode(JSON.stringify(agentData));

      // Create metadata matching AgentEntityMetadata interface
      const metadata: AgentEntityMetadata = {
        agentLoopId: entity.id,
        status: entity.getStatus(),
        createdAt: entity.state.startTime || Date.now(),
        updatedAt: entity.state.endTime || undefined,
        completedAt: entity.state.endTime || undefined,
        tags: [],
        customFields: {
          currentIteration: entity.state.currentIteration,
          toolCallCount: entity.state.toolCallCount,
          maxIterations: entity.config?.maxIterations,
        },
      };

      await this.storageAdapter.save(entity.id, data, metadata);
      logger.debug("Agent loop persisted to storage", {
        agentLoopId: entity.id,
        status: entity.getStatus(),
      });
    } catch (error) {
      // Log error but don't throw - storage failure should not affect core functionality
      logger.error("Failed to persist agent loop to storage", {
        agentLoopId: entity.id,
        error: getErrorMessage(error),
      });
    }
  }

  /**
   * Remove agent loop from storage (if adapter is available)
   * @param agentLoopId Agent loop ID to remove
   */
  private async removeFromStorage(agentLoopId: string): Promise<void> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping agent loop removal from storage");
      return;
    }

    try {
      await this.storageAdapter.delete(agentLoopId);
      logger.debug("Agent loop removed from storage", { agentLoopId });
    } catch (error) {
      // Log error but don't throw - storage failure should not affect core functionality
      logger.error("Failed to remove agent loop from storage", {
        agentLoopId,
        error: getErrorMessage(error),
      });
    }
  }

  /**
   * Load agent loop data from storage (if adapter is available)
   * 
   * IMPORTANT: This method loads raw serialized data, NOT a fully reconstructed AgentLoopEntity.
   * To restore a complete entity with all dependencies (config, conversationManager, etc.),
   * use AgentLoopFactory.fromCheckpoint() which properly handles:
   * - Loading checkpoint from storage
   * - Extracting state snapshot
   * - Reconstructing entity with re-provided config
   * - Restoring conversation manager and other runtime components
   * 
   * @param agentLoopId Agent loop ID to load
   * @returns Raw agent loop data or null (cannot be used directly as AgentLoopEntity)
   */
  private async _loadFromStorage(agentLoopId: string): Promise<unknown | null> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, cannot load from storage");
      return null;
    }

    try {
      const data = await this.storageAdapter.load(agentLoopId);
      if (!data) {
        logger.debug("No data found in storage for agent loop", { agentLoopId });
        return null;
      }

      // Deserialize agent loop data
      const decoder = new TextDecoder();
      const agentData = JSON.parse(decoder.decode(data));
      
      logger.debug("Raw agent loop data loaded from storage", { agentLoopId });
      return agentData;
    } catch (error) {
      logger.error("Failed to load agent loop data from storage", {
        agentLoopId,
        error: getErrorMessage(error),
      });
      return null;
    }
  }

  /**
   * Initialize registry from storage (preload all agent loops)
   * Note: This is typically not used for agent loops as they are created dynamically.
   * This method is provided for completeness and potential future use cases.
   */
  async initializeFromStorage(): Promise<void> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping initialization from storage");
      return;
    }

    try {
      const agentLoopIds = await this.storageAdapter.list();
      logger.info("Found agent loops in storage", { count: agentLoopIds.length });
      
      // Note: We don't automatically load all agent loops into memory
      // Agent loops are typically loaded on-demand when needed
      // This method can be extended to implement caching strategies if needed
    } catch (error) {
      logger.error("Failed to initialize agent loop registry from storage", {
        error: getErrorMessage(error),
      });
    }
  }
}
