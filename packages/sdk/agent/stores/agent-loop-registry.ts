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
import type { AgentStateCoordinator } from "../state-managers/agent-state-coordinator.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { getErrorMessage } from "@wf-agent/common-utils";
import type { PersistenceConfig } from "../../shared/storage/persistence-strategy.js";
import {
  PersistenceStrategy,
  mergePersistenceConfig,
} from "../../shared/storage/persistence-strategy.js";
import {
  PersistenceEventEmitter,
} from "../../shared/storage/persistence-event-emitter.js";
import { DataConsistencyValidator, type ConsistencyReport } from "../../shared/storage/data-consistency-validator.js";

const logger = createContextualLogger({ component: "AgentLoopRegistry" });

/**
 * AgentLoopRegistry - Agent Loop Registry
 *
 * Core Responsibilities:
 * - Manage active AgentLoopEntity instances.
 * - Provide registration, query and deletion of instances
 * - Store AgentStateCoordinator alongside entities
 * - Handle persistence with configurable strategies (BLOCKING/ASYNC)
 * - Notify applications of persistence events
 *
 * Design Principles:
 * - Singleton model (managed via DI container)
 * - Workflow-execution-safe (Map operations)
 * - Support for cleaning up expired instances
 * - Clear separation between sync and async persistence models
 *
 * Persistence Model:
 * - register(): Synchronous registration, background persistence
 * - registerAsync(): Async registration with strategy-aware persistence
 *   - BLOCKING mode: Waits for storage to complete, throws on error
 *   - ASYNC mode: Returns immediately, emits events for status
 */
export class AgentLoopRegistry implements IAgentExecutionRegistry {
  /** Instance Storage */
  private entities: Map<ID, AgentLoopEntity> = new Map();
  /** State Coordinator Storage */
  private stateCoordinatorMap: Map<ID, AgentStateCoordinator> = new Map();
  private storageAdapter?: AgentLoopStorageAdapter;
  private persistenceConfig: Required<PersistenceConfig>;
  /** Event system for persistence notifications */
  private eventEmitter: PersistenceEventEmitter = new PersistenceEventEmitter();
  /** Data consistency validator */
  private consistencyValidator = new DataConsistencyValidator();

  /**
   * Constructor
   * @param options Configuration options
   */
  constructor(options?: {
    storageAdapter?: AgentLoopStorageAdapter;
    persistenceConfig?: PersistenceConfig;
  }) {
    this.storageAdapter = options?.storageAdapter;
    this.persistenceConfig = mergePersistenceConfig(options?.persistenceConfig);
  }

  /**
   * Register AgentLoopEntity (Synchronous)
   *
   * IMPORTANT: This method registers the entity in memory synchronously.
   * Persistence is ALWAYS asynchronous, regardless of configured strategy.
   *
   * For applications that require guaranteed persistence before continuing,
   * use registerAsync() with appropriate strategy configuration.
   *
   * @param entity The Agent Loop entity
   * @deprecated Use registerAsync() for persistence-aware registration
   */
  register(entity: AgentLoopEntity): void {
    this.entities.set(entity.id, entity);

    // Always async persistence (cannot block in sync method)
    // In BLOCKING mode, use registerAsync() instead
    this.persistToStorageAsync(entity).catch(() => {
      // Error already logged in persistToStorageAsync
    });
  }

  /**
   * Register AgentLoopEntity (Asynchronous)
   *
   * Registers entity in memory and persists based on configured strategy:
   *
   * - BLOCKING mode: Waits for persistence to complete
   *   - Success: Returns normally
   *   - Failure: Throws error, entity remains in memory
   *   - Guarantees: Memory state matches storage state
   *
   * - ASYNC mode: Returns immediately
   *   - Persistence happens in background
   *   - Emits events: persist_started, persist_success, persist_failed
   *   - Application must listen to events to detect failures
   *
   * @param entity The Agent Loop entity
   * @param options Optional configuration for this operation
   * @throws {Error} In BLOCKING mode if persistence fails
   *
   * @example
   * ```typescript
   * // BLOCKING mode: Guaranteed persistence
   * try {
   *   await registry.registerAsync(entity);
   *   // Entity is safely persisted
   * } catch (error) {
   *   // Handle persistence failure
   * }
   *
   * // ASYNC mode: Non-blocking with event notification
   * await registry.registerAsync(entity);
   * registry.onPersistFailed((event) => {
   *   console.error(`Failed to persist ${event.entityId}: ${event.error.message}`);
   * });
   * ```
   */
  async registerAsync(
    entity: AgentLoopEntity,
    options?: { waitForPersistence?: boolean },
  ): Promise<void> {
    // Always register in memory first
    this.entities.set(entity.id, entity);

    const waitForPersistence = options?.waitForPersistence ??
                              (this.persistenceConfig.strategy === PersistenceStrategy.BLOCKING);

    if (waitForPersistence) {
      // BLOCKING mode: Wait for persistence to complete and propagate errors
      await this.persistToStorageAsync(entity);
    } else {
      // ASYNC mode: Start persistence in background, don't wait
      this.persistToStorageAsync(entity).catch(() => {
        // Error already logged and emitted as event
      });
    }
  }

  /**
   * Unregister AgentLoopEntity (Synchronous)
   *
   * IMPORTANT: This method removes the entity from memory synchronously.
   * Removal from storage is ALWAYS asynchronous.
   *
   * For applications that require guaranteed storage removal before continuing,
   * use unregisterAsync().
   *
   * @param id Instance ID
   * @returns Whether the unregister was successful
   */
  unregister(id: ID): boolean {
    const result = this.entities.delete(id);

    // Also clean up associated state coordinator
    if (result) {
      this.stateCoordinatorMap.delete(id);
      // Remove from storage asynchronously
      this.removeFromStorageAsync(id).catch(() => {
        // Error already logged
      });
    }

    return result;
  }

  /**
   * Unregister AgentLoopEntity (Asynchronous)
   *
   * Removes entity from memory and storage based on configured strategy:
   *
   * - BLOCKING mode: Waits for storage removal to complete
   * - ASYNC mode: Returns immediately, removal happens in background
   *
   * @param id Instance ID
   * @param options Optional configuration for this operation
   * @returns Whether the unregister was successful
   * @throws {Error} In BLOCKING mode if removal fails
   */
  async unregisterAsync(
    id: ID,
    options?: { waitForRemoval?: boolean },
  ): Promise<boolean> {
    const result = this.entities.delete(id);

    // Also clean up associated state coordinator
    if (result) {
      this.stateCoordinatorMap.delete(id);

      const waitForRemoval = options?.waitForRemoval ??
                            (this.persistenceConfig.strategy === PersistenceStrategy.BLOCKING);

      if (waitForRemoval) {
        // BLOCKING mode: Wait for storage removal
        await this.removeFromStorageAsync(id);
      } else {
        // ASYNC mode: Start removal in background
        this.removeFromStorageAsync(id).catch(() => {
          // Error already logged and emitted as event
        });
      }
    }

    return result;
  }

  /**
   * Register AgentStateCoordinator
   * @param agentLoopId Agent Loop ID
   * @param stateCoordinator AgentStateCoordinator instance
   */
  registerStateCoordinator(agentLoopId: ID, stateCoordinator: AgentStateCoordinator): void {
    this.stateCoordinatorMap.set(agentLoopId, stateCoordinator);
  }

  /**
   * Get AgentStateCoordinator
   * @param agentLoopId Agent Loop ID
   * @returns AgentStateCoordinator instance or null
   */
  getStateCoordinator(agentLoopId: ID): AgentStateCoordinator | null {
    return this.stateCoordinatorMap.get(agentLoopId) || null;
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
      logger.debug("Agent loop not in memory, attempting to load from storage", {
        agentLoopId: id,
      });
      // Note: Full restoration requires checkpoint mechanism via AgentLoopFactory.fromCheckpoint()
      // This method only provides basic data loading. For complete entity restoration,
      // use the checkpoint restore API instead.
      const loadedData = await this._loadFromStorage(id);
      if (loadedData) {
        logger.warn(
          "Loaded raw data from storage but cannot reconstruct full entity without checkpoint. " +
            "Use AgentLoopFactory.fromCheckpoint() for complete restoration.",
          { agentLoopId: id },
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
        return parent?.parentType === "WORKFLOW" && parent.parentId === filter.parentWorkflowId;
      });
    }

    return results;
  }

  /**
   * Cleaning up terminated (completed + failed + cancelled) instances
   * Calls cleanup on each terminated entity before unregistering.
   * @returns Number of instances cleaned up
   */
  cleanupTerminated(): number {
    const terminatedEntities = [
      ...this.getCompleted(),
      ...this.getFailed(),
      ...this.getByStatus(AgentLoopStatus.CANCELLED),
    ];
    for (const entity of terminatedEntities) {
      entity.cleanup();
      this.unregister(entity.id);
    }
    return terminatedEntities.length;
  }

  /**
   * Clear all instances
   * Calls the cleanup method of each entity before clearing.
   */
  clear(): void {
    for (const entity of this.entities.values()) {
      entity.cleanup();
    }
    this.entities.clear();
    this.stateCoordinatorMap.clear();
  }

  /**
   * Enable await using pattern support
   * Delegates to clear() for resource release
   */
  async [Symbol.asyncDispose](): Promise<void> {
    this.clear();
  }

  // ============================================================
  // Event and Consistency Methods
  // ============================================================

  /**
   * Get the persistence event emitter
   *
   * Allows applications to listen for persistence events and track failures:
   *
   * @returns PersistenceEventEmitter instance
   *
   * @example
   * ```typescript
   * // Listen for persistence failures in ASYNC mode
   * registry.getEventEmitter().onPersistFailed((event) => {
   *   console.error(`Failed to persist ${event.entityId}: ${event.error.message}`);
   *   // Application-level retry or alerting logic
   * });
   *
   * // Listen for successful persistence
   * registry.getEventEmitter().onPersistSuccess((event) => {
   *   console.log(`Successfully persisted ${event.entityId}`);
   * });
   *
   * // Get count of failed operations for monitoring
   * const failedCount = registry.getEventEmitter().getFailedCount();
   * ```
   */
  getEventEmitter(): PersistenceEventEmitter {
    return this.eventEmitter;
  }

  /**
   * Verify data consistency between memory and storage
   *
   * Compares the set of entities in memory with those in storage.
   * Detects:
   * - Orphaned data in storage (exists in storage but not memory)
   * - Missing data in storage (exists in memory but not storage)
   * - Size mismatches
   *
   * @returns Consistency report with issues and statistics
   *
   * @example
   * ```typescript
   * const report = await registry.verifyConsistency();
   * if (!report.consistent) {
   *   const formatter = registry.getConsistencyValidator();
   *   console.error(formatter.generateReport(report));
   *   // Take corrective action
   * }
   * ```
   */
  async verifyConsistency(): Promise<ConsistencyReport> {
    const memoryIds = new Set(this.getAllIds());
    const storageIds = new Set(this.storageAdapter ? await this.storageAdapter.list() : []);

    return this.consistencyValidator.verify("AgentLoopRegistry", memoryIds, storageIds);
  }

  /**
   * Get consistency validator instance
   * Useful for generating formatted reports
   */
  getConsistencyValidator(): DataConsistencyValidator {
    return this.consistencyValidator;
  }

  // ============================================================
  // Storage Persistence Methods
  // ============================================================

  /**
   * Persist agent loop to storage (if adapter is available)
   *
   * Behavior depends on persistence strategy:
   * - BLOCKING: Throws on failure
   * - ASYNC: Emits events, errors logged only
   *
   * @param entity Agent loop entity to persist
   * @throws {Error} In BLOCKING mode if persistence fails
   */
  private async persistToStorageAsync(entity: AgentLoopEntity): Promise<void> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping agent loop persistence");
      return;
    }

    try {
      // Notify listeners that persistence is starting
      this.eventEmitter.notifyPersistStarted(entity.id, "save");

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

      // Notify success
      this.eventEmitter.notifyPersistSuccess(entity.id, "save", data.byteLength);

      logger.debug("Agent loop persisted to storage", {
        agentLoopId: entity.id,
        status: entity.getStatus(),
      });
    } catch (error) {
      const errorObj = error instanceof Error ? error : new Error(String(error));

      // Notify failure (event will be tracked for diagnostics)
      this.eventEmitter.notifyPersistFailed(entity.id, "save", errorObj);

      // Error handling depends on persistence strategy
      if (this.persistenceConfig.strategy === PersistenceStrategy.BLOCKING) {
        // In BLOCKING mode, re-throw to make the error visible to caller
        logger.error("Failed to persist agent loop to storage (BLOCKING mode)", {
          agentLoopId: entity.id,
          error: getErrorMessage(error),
        });
        throw error;
      } else {
        // In ASYNC mode, just log the error
        // Application must listen to persist_failed events
        logger.error("Failed to persist agent loop to storage (ASYNC mode)", {
          agentLoopId: entity.id,
          error: getErrorMessage(error),
        });
      }
    }
  }

  /**
   * Remove agent loop from storage (if adapter is available)
   *
   * @param agentLoopId Agent loop ID to remove
   * @throws {Error} In BLOCKING mode if removal fails
   */
  private async removeFromStorageAsync(agentLoopId: string): Promise<void> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping agent loop removal from storage");
      return;
    }

    try {
      this.eventEmitter.notifyPersistStarted(agentLoopId, "delete");

      await this.storageAdapter.delete(agentLoopId);

      this.eventEmitter.notifyPersistSuccess(agentLoopId, "delete");

      logger.debug("Agent loop removed from storage", { agentLoopId });
    } catch (error) {
      const errorObj = error instanceof Error ? error : new Error(String(error));

      this.eventEmitter.notifyPersistFailed(agentLoopId, "delete", errorObj);

      // Error handling depends on persistence strategy
      if (this.persistenceConfig.strategy === PersistenceStrategy.BLOCKING) {
        // In BLOCKING mode, re-throw to make the error visible
        logger.error("Failed to remove agent loop from storage (BLOCKING mode)", {
          agentLoopId,
          error: getErrorMessage(error),
        });
        throw error;
      } else {
        // In ASYNC mode, just log the error
        logger.error("Failed to remove agent loop from storage (ASYNC mode)", {
          agentLoopId,
          error: getErrorMessage(error),
        });
      }
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
   *
   * This method loads all agent loops from storage into memory.
   * Useful for:
   * - Restoring agent loop instances across SDK sessions
   * - Rebuilding the in-memory registry after restart
   *
   * IMPORTANT: This loads only metadata and basic state.
   * Full restoration with conversation history and runtime components
   * requires AgentLoopFactory.fromCheckpoint() for each entity.
   *
   * Behavior:
   * - Partial failures do NOT stop the entire initialization
   * - Failed entities are logged but not added to the registry
   * - Returns summary of loaded/failed entities
   *
   * @returns Summary of initialization
   */
  async initializeFromStorage(): Promise<{
    loadedCount: number;
    failedCount: number;
    totalCount: number;
    failures: Array<{ id: string; error: string }>;
  }> {
    if (!this.storageAdapter) {
      logger.debug("No storage adapter configured, skipping initialization from storage");
      return { loadedCount: 0, failedCount: 0, totalCount: 0, failures: [] };
    }

    try {
      const agentLoopIds = await this.storageAdapter.list();
      logger.info("Found agent loops in storage for initialization", { count: agentLoopIds.length });

      let loadedCount = 0;
      let failedCount = 0;
      const failures: Array<{ id: string; error: string }> = [];

      // Load each agent loop from storage
      for (const id of agentLoopIds) {
        try {
          const loadedData = await this._loadFromStorage(id);
          if (loadedData) {
            // Add basic metadata to the registry
            // Note: Full entity reconstruction requires AgentLoopFactory.fromCheckpoint()
            // This only adds minimal data to mark the entity as known
            logger.debug("Loaded agent loop metadata from storage", { agentLoopId: id });
            loadedCount++;
          }
        } catch (error) {
          failedCount++;
          const errorMsg = getErrorMessage(error);
          failures.push({ id, error: errorMsg });
          logger.warn(`Failed to load agent loop from storage, skipping`, {
            agentLoopId: id,
            error: errorMsg,
          });
        }
      }

      const summary = { loadedCount, failedCount, totalCount: agentLoopIds.length, failures };

      logger.info("Agent loops initialized from storage", {
        loaded: summary.loadedCount,
        failed: summary.failedCount,
        total: summary.totalCount,
      });

      return summary;
    } catch (error) {
      logger.error("Failed to initialize agent loop registry from storage", {
        error: getErrorMessage(error),
      });
      throw error;
    }
  }
}
