/**
 * Agent Loop Checkpoint Coordinator
 *
 * A service that coordinates the entire checkpoint process
 * Extends BaseCheckpointCoordinator to eliminate code duplication.
 */

import { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import type { ConversationSession } from "../../shared/messaging/conversation-session.js";
import type {
  CheckpointMetadata,
  DeltaStorageConfig,
  TCheckpointType,
  AgentLoopCheckpoint,
  AgentLoopStateSnapshot,
  AgentLoopRuntimeConfig,
  DeltaCheckpoint,
  FullCheckpoint,
  AgentLoopDelta,
  AgentCheckpointContentConfig,
  CheckpointFormatVersion,
  CheckpointErrorStrategy,
  CheckpointErrorContext,
  ExecutionHierarchyMetadata,
} from "@wf-agent/types";
import { AgentCheckpointError, CURRENT_CHECKPOINT_FORMAT_VERSION } from "@wf-agent/types";
import { BaseCheckpointCoordinator } from "../../shared/checkpoint/base-checkpoint-coordinator.js";
import type { CheckpointDependencies as BaseCheckpointDependencies } from "../../shared/checkpoint/types.js";
import { BaseDeltaRestorer } from "../../shared/checkpoint/base-delta-restorer.js";
import { CheckpointVersionManager } from "../../shared/checkpoint/checkpoint-version-manager.js";
import { CheckpointErrorHandler } from "../../shared/checkpoint/checkpoint-error-handler.js";
import { buildCheckpointMetadata } from "../../shared/checkpoint/utils/metadata-builder.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { getExecutionEventBus } from "../../shared/events/index.js";
import type { FileCheckpointManager } from "@wf-agent/common-utils";
import type { ExecutionHierarchyRegistry, AnyExecutionEntity } from "../../shared/registry/execution-hierarchy-registry.js";
import { HierarchyIntegrityService } from "../../shared/execution/hierarchy-integrity-service.js";
import type { ChildCheckpointResolver } from "../../shared/checkpoint/child-checkpoint-resolver.js";
import { ChildCheckpointRestorer } from "../../shared/checkpoint/child-checkpoint-restorer.js";
import type { ChildRestoreDependencies } from "../../shared/checkpoint/child-checkpoint-restorer.js";
import type { CheckpointCoordinator, WorkflowCheckpointDependencies } from "../../workflow/checkpoint/checkpoint-coordinator.js";
import { RestoreStrategyRegistry } from "../../shared/checkpoint/restore-strategy.js";

const logger = createContextualLogger({ component: "AgentLoopCheckpointCoordinator" });

/**
 * Checkpoint creation options
 */
export interface CheckpointOptions {
  /** Checkpoint metadata */
  metadata?: CheckpointMetadata;
  /** Optional description */
  description?: string;
  /** Optional tags */
  tags?: string[];
  /** Content configuration for what to include in checkpoint */
  contentConfig?: AgentCheckpointContentConfig;
}

   /**
    * Checkpoint dependencies (extends base with agent-specific fields)
    */
   export interface CheckpointDependencies extends BaseCheckpointDependencies<AgentLoopCheckpoint> {
     /** Save checkpoints */
     saveCheckpoint: (checkpoint: AgentLoopCheckpoint) => Promise<string>;
     /** Get checkpoints */
     getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>;
     /** List the checkpoints */
     listCheckpoints: (agentLoopId: string) => Promise<string[]>;
     /** Incremental storage configuration (optional) */
     deltaConfig?: DeltaStorageConfig;
     /** Conversation manager for message persistence (optional) */
     conversationManager?: ConversationSession;
     /** File checkpoint manager for persisting external file state (optional) */
     fileCheckpointManager?: FileCheckpointManager;
     /** Hierarchy registry for child execution restoration (optional) */
     hierarchyRegistry?: ExecutionHierarchyRegistry;
     /** Child checkpoint resolver for finding latest child checkpoints (optional) */
     childCheckpointResolver?: ChildCheckpointResolver;
     /** Workflow coordinator for restoring WORKFLOW children */
     workflowCoordinator?: CheckpointCoordinator;
     /** Workflow dependencies for restoring WORKFLOW children */
     workflowDeps?: WorkflowCheckpointDependencies;
     /** Custom checkpoint resolver for child restoration */
     childCheckpointResolverForRestore?: ChildCheckpointResolver;
   }

interface AgentRestoreContext {
  entity: AgentLoopEntity;
  checkpoint: AgentLoopCheckpoint;
  restoredSnapshot: AgentLoopStateSnapshot;
}

/**
 * Agent Loop checkpoint coordinator
 *
 * Design Principles:
 * - Instance-based for dependency injection and testability
 * - Coordinates the entire checkpoint lifecycle
 * - Extends BaseCheckpointCoordinator to eliminate duplication
 * - Config must be provided at construction time (via constructor) for restore operations
 *   because AgentLoopRuntimeConfig contains callbacks that cannot be serialized
 */
export class AgentLoopCheckpointCoordinator extends BaseCheckpointCoordinator<
  AgentLoopCheckpoint,
  AgentLoopEntity,
  AgentLoopStateSnapshot
> {
  /**
    * Config for restore operations.
    * Required because AgentLoopRuntimeConfig contains callbacks that cannot be serialized.
    * Must be set before calling restoreFromCheckpoint().
    */
  private restoreConfig?: AgentLoopRuntimeConfig;

   /**
    * Child restorer for cross-type restoration.
    * Optional. If provided, enables automatic restoration of child executions.
    */
   private childRestoreComponent?: ChildCheckpointRestorer;

  /**
    * Version manager for format compatibility and migration
    */
  private versionManager: CheckpointVersionManager;

  /**
   * Error handler for checkpoint operations
   */
  private checkpointErrorHandler?: CheckpointErrorHandler;

  /**
   * Error handling strategy
   */
  private checkpointErrorStrategy: CheckpointErrorStrategy = "warn";

  /**
   * @param config AgentLoopRuntimeConfig for restoration (must be provided for restore operations)
   */
  constructor(config?: AgentLoopRuntimeConfig) {
    super();
    this.restoreConfig = config;
    this.versionManager = new CheckpointVersionManager(logger);
  }

  /**
    * Set config for restore operations
    * @param config AgentLoopRuntimeConfig for restoration
    */
   setConfig(config: AgentLoopRuntimeConfig): void {
     this.restoreConfig = config;
   }

   /**
     * Set child restorer for cross-type restoration.
     * When provided, enables automatic restoration of child executions
     * during agent loop checkpoint restore.
     * @param restorer ChildCheckpointRestorer instance
     */
    setChildRestoreComponent(restorer: ChildCheckpointRestorer): void {
      this.childRestoreComponent = restorer;
    }
   /**
     * Create a checkpoint
     * @param entity Agent Loop entity
     * @param dependencies dependencies
     * @param options checkpoint options
     * @param context optional context (e.g., contentConfig)
     *          Pass contentConfig via context to control what state to include in checkpoint
     * @returns checkpoint ID
     */
   override async createCheckpoint(
     entity: AgentLoopEntity,
     dependencies: CheckpointDependencies,
     options?: CheckpointOptions,
     context?: Record<string, unknown>,
   ): Promise<string> {
     const mergedMetadata = buildCheckpointMetadata({
       metadata: options?.metadata,
       description: options?.description,
       tags: options?.tags,
     });

     // Build context with contentConfig from options or context parameter
     const mergedContext: Record<string, unknown> = context ?? {};
     const contentConfig = (context as { contentConfig?: AgentCheckpointContentConfig })?.contentConfig
       ?? options?.contentConfig;
     if (contentConfig) {
       mergedContext['contentConfig'] = contentConfig;
     }

     const checkpointId = await super.createCheckpoint(
       entity,
       dependencies,
       mergedMetadata,
       mergedContext,
     );

    // Publish checkpoint state change event (Plan C)
    const eventBus = getExecutionEventBus();
    await eventBus.publish({
      type: "state_changed",
      executionId: entity.id,
      timestamp: Date.now(),
      previousStatus: entity.state.status,
      newStatus: entity.state.status,
      changes: {
        checkpointCreated: checkpointId,
        description: options?.description,
        tags: options?.tags,
      },
    });

    // Create file checkpoint if manager is available
    if (dependencies.fileCheckpointManager) {
      try {
        await dependencies.fileCheckpointManager.createCheckpoint(entity.id);
        logger.info("File checkpoint created alongside agent loop checkpoint", {
          agentLoopId: entity.id,
          checkpointId,
        });
      } catch (error) {
        await this.handleFileCheckpointError(
          error instanceof Error ? error : new Error(String(error)),
          "create",
          entity.id,
          checkpointId,
        );
      }
    }

    return checkpointId;
  }

  /**
   * Restore Agent Loop entity from checkpoint
   * @param checkpointId checkpointId
   * @param dependencies dependencies
   * @param runtimeConfig AgentLoopRuntimeConfig for restoration (REQUIRED)
   * @returns Recovered Agent Loop Entity
   */
  async restoreAgentLoopFromCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies,
    runtimeConfig: AgentLoopRuntimeConfig,
  ): Promise<AgentLoopEntity> {
    this.restoreConfig = runtimeConfig;

    try {
      const checkpoint = await dependencies.getCheckpoint(checkpointId);
      if (!checkpoint) {
        throw new Error("Checkpoint not found");
      }

      const formatVersion = (checkpoint.metadata?.customFields?.["formatVersion"] as CheckpointFormatVersion) || CURRENT_CHECKPOINT_FORMAT_VERSION;

      const compatibility = this.versionManager.checkCompatibility(formatVersion);
      if (!compatibility.compatible) {
        throw new Error(`Checkpoint version not compatible: ${compatibility.reason}`);
      }

      if (compatibility.requiresMigration) {
        logger.info("Checkpoint requires migration, starting migration process", {
          checkpointId,
          reason: compatibility.reason,
        });
        const migrationResult = await this.versionManager.migrateCheckpoint(checkpoint);
        if (!migrationResult.success) {
          throw new Error(`Checkpoint migration failed: ${migrationResult.errors?.join(", ")}`);
        }
      }

      let agentLoopState: AgentLoopStateSnapshot;
      if (checkpoint.type === "DELTA") {
         const metadataLoader = async (entityId: string, _entityType: string) => {
           const records = await dependencies.listCheckpoints(entityId);
           const checkpoints: Array<{ id: string; metadata: { previousCheckpointId?: string; checkpointType?: string; timestamp: number; chainRootId?: string; chainPosition?: number } }> = [];
           for (const id of records) {
             const cp = await dependencies.getCheckpoint(id);
             if (cp) {
               checkpoints.push({
                 id,
                 metadata: cp.metadata as { previousCheckpointId?: string; checkpointType?: string; timestamp: number; chainRootId?: string; chainPosition?: number },
               });
             }
           }
           return checkpoints
             .sort((a, b) => a.metadata.timestamp - b.metadata.timestamp)
             .map(c => ({
               id: c.id,
               previousCheckpointId: c.metadata.previousCheckpointId,
               checkpointType: c.metadata.checkpointType as "FULL" | "DELTA",
               timestamp: c.metadata.timestamp,
               chainRootId: c.metadata.chainRootId,
               chainPosition: c.metadata.chainPosition,
             }));
         };

         const restorer = new BaseDeltaRestorer<AgentLoopCheckpoint, AgentLoopStateSnapshot>(
           id => dependencies.getCheckpoint(id),
           ids => dependencies.getCheckpoints?.(ids) ?? Promise.resolve(new Map()),
           metadataLoader,
         );
         const restoreResult = await restorer.restore(checkpointId);
         agentLoopState = restoreResult.snapshot;
       } else {
         const fullCp = checkpoint as FullCheckpoint<AgentLoopStateSnapshot>;
         agentLoopState = fullCp.snapshot;
       }

      const entity = AgentLoopEntity.fromSnapshot(checkpoint.agentLoopId, agentLoopState, this.restoreConfig);

      const hierarchyFromSnapshot = (agentLoopState as unknown as Record<string, unknown>)['hierarchy'] as ExecutionHierarchyMetadata | undefined;
      if (hierarchyFromSnapshot) {
        entity.restoreHierarchy(hierarchyFromSnapshot);
      }

      const restoreCtx: AgentRestoreContext = {
        entity,
        checkpoint,
        restoredSnapshot: agentLoopState,
      };

      await this.postRestore(entity, dependencies, restoreCtx);

      const eventBus = getExecutionEventBus();
      await eventBus.publish({
        type: "state_changed",
        executionId: entity.id,
        timestamp: Date.now(),
        newStatus: entity.state.status,
        changes: {
          restored: true,
          checkpointId,
        },
      });

      return entity;
    } catch (error) {
      if (error instanceof Error && error.message.includes("Checkpoint not found")) {
        throw new AgentCheckpointError(
          `Checkpoint not found: ${checkpointId}`,
          "restore",
          checkpointId,
        );
      }
      throw error;
    }
  }

   private async postRestore(
     entity: AgentLoopEntity,
     dependencies: CheckpointDependencies,
     restoreCtx: AgentRestoreContext,
   ): Promise<void> {
     const { hierarchyRegistry } = dependencies;

     if (hierarchyRegistry) {
       hierarchyRegistry.register(entity);
     }

      // Restore child executions using ChildCheckpointRestorer if available
      if (this.childRestoreComponent) {
        const childRefs = entity.getChildReferences();
        if (childRefs.length > 0) {
          logger.info("Restoring child executions for agent loop", {
            agentLoopId: entity.id,
            childCount: childRefs.length,
          });

           const restoreDeps = await this.buildChildRestoreDependencies(dependencies, hierarchyRegistry);
           const results = await this.childRestoreComponent.restoreChildren(
             entity as AnyExecutionEntity,
             childRefs,
             restoreDeps,
             dependencies.childCheckpointResolverForRestore,
           );

          const summary = ChildCheckpointRestorer.summarizeResults(results);
         if (summary.failed > 0) {
           logger.warn("Some child executions failed to restore", {
             agentLoopId: entity.id,
             ...summary,
           });
         }
       }
     } else {
        logger.debug("ChildCheckpointRestorer not configured, skipping child restoration", {
         agentLoopId: entity.id,
         childCount: entity.getChildReferences().length,
       });
     }

     // Validate hierarchy integrity
     if (hierarchyRegistry) {
       const hierarchyMetadata = entity.getHierarchyMetadata();
       if (hierarchyMetadata) {
         const validation = HierarchyIntegrityService.validateIntegrity(
           hierarchyMetadata,
           hierarchyRegistry,
         );

         if (!validation.valid) {
           logger.warn("Hierarchy integrity issues detected after checkpoint restore", {
             agentLoopId: entity.id,
             issues: validation.issues,
           });
         }
       }
     }

     // Restore file checkpoint
     if (dependencies.fileCheckpointManager) {
       try {
         const fileCheckpoints = await dependencies.fileCheckpointManager
           .getStorage()
           .listByEntity(entity.id, { limit: 1 });
         if (fileCheckpoints.length > 0) {
           const result = await dependencies.fileCheckpointManager.restoreCheckpoint(
             entity.id,
             fileCheckpoints[0]!.id,
           );
           logger.info("File checkpoint restored alongside agent loop checkpoint", {
             agentLoopId: entity.id,
             checkpointId: restoreCtx.checkpoint.id,
             restoredCount: result.restoredCount,
             deletedCount: result.deletedCount,
             skippedCount: result.skippedCount,
           });
         }
       } catch (error) {
         await this.handleFileCheckpointError(
           error instanceof Error ? error : new Error(String(error)),
           "restore",
           entity.id,
           restoreCtx.checkpoint.id,
         );
       }
     }
   }

     /**
      * Build ChildRestoreDependencies for the current restore operation.
      * Supports both AGENT_LOOP and WORKFLOW child types via workflow coordinator.
      */
     private async buildChildRestoreDependencies(
       dependencies: CheckpointDependencies,
       hierarchyRegistry: ExecutionHierarchyRegistry | undefined,
     ): Promise<ChildRestoreDependencies> {
       const strategyRegistry = new RestoreStrategyRegistry();
       const self = this;

       strategyRegistry.register({
         executionType: "AGENT_LOOP",
         findCheckpoint: async (childId) => {
           const ids = await dependencies.listCheckpoints(childId);
           return ids.length > 0 ? ids[ids.length - 1] : undefined;
         },
          restoreEntity: async (checkpointId, _parentId) => {
            const entity = await self.restoreAgentLoopFromCheckpoint(
              checkpointId,
              dependencies,
              self.restoreConfig!,
            );
            return entity as unknown as AnyExecutionEntity;
          },
         registerChild: (parent, child, childRef) => {
           parent.registerChild(childRef);
           if (hierarchyRegistry) {
             hierarchyRegistry.register(child as AnyExecutionEntity);
           }
         },
       });

       if (dependencies.workflowCoordinator && dependencies.workflowDeps) {
         const workflowDeps = dependencies.workflowDeps;
         strategyRegistry.register({
           executionType: "WORKFLOW",
           findCheckpoint: async (childId) => {
             const ids = await workflowDeps.checkpointStateManager.list({
               parentId: childId,
             });
             return ids.length > 0 ? ids[ids.length - 1] : undefined;
           },
            restoreEntity: async (checkpointId, _parentId) => {
              const result = await dependencies.workflowCoordinator!.restoreWorkflowFromCheckpoint(
                checkpointId,
                workflowDeps,
              );
              return result.workflowExecutionEntity as AnyExecutionEntity;
            },
           registerChild: (parent, child, childRef) => {
             parent.registerChild(childRef);
             if (hierarchyRegistry) {
               hierarchyRegistry.register(child as AnyExecutionEntity);
             }
           },
         });
       }

       return {
         findCheckpoint: async (childId, childType) => {
           const strategy = strategyRegistry.get(childType);
           if (strategy) {
             return strategy.findCheckpoint(childId);
           }
           return undefined;
         },
         restoreEntity: async (checkpointId, childType, parentId) => {
           const strategy = strategyRegistry.get(childType);
           if (strategy) {
             return strategy.restoreEntity(checkpointId, parentId);
           }
           throw new Error(`No strategy registered for type: ${childType}`);
         },
         registerChild: (parent, child, childRef) => {
           const strategy = strategyRegistry.get(childRef.childType);
           if (strategy) {
             strategy.registerChild(parent, child, childRef);
           }
         },
         strategyRegistry,
         onChildRestored: undefined,
       };
     }

  // ============================================================================
  // Abstract Methods Implementation
  // ============================================================================

  /**
   * Extracting a state snapshot
   *
   * Only serializes persistent execution progress data (iteration count, status, tool calls).
   * Does NOT include:
   * - `config`: Contains callbacks, must be re-provided by application on restore
   * - `messages`: Managed by ConversationSession, not AgentLoopState
   *
   * Respects content filtering options from AgentCheckpointContentConfig:
   * - includeState: Whether to include status, iteration count, etc. (default: true)
   * - includeMessages: Whether to include message history (default: false)
   * - messageLimit: Max number of messages to include
   * - includeToolCalls: Whether to include tool call records (default: true)
   * - toolCallLimit: Max number of tool calls to include
   *
   * Plan C: Includes execution records (errors, interruptions, events) that are
   * part of AgentLoopState and automatically serialized to checkpoint.
   *
   * @param entity Agent Loop entity
   * @returns Status Snapshot
   */
  protected extractState(
    entity: AgentLoopEntity,
    context?: { contentConfig?: AgentCheckpointContentConfig },
  ): AgentLoopStateSnapshot {
    const contentConfig = context?.contentConfig;

    // Get the full snapshot from AgentLoopState (includes all new Plan C fields)
    const fullSnapshot = entity.state.createSnapshot();
    const snapshot: Record<string, unknown> = {};

    // Include execution state by default
    if (contentConfig?.includeState !== false) {
      snapshot['status'] = fullSnapshot.status;
      snapshot['currentIteration'] = fullSnapshot.currentIteration;
      snapshot['toolCallCount'] = fullSnapshot.toolCallCount;
      snapshot['startTime'] = fullSnapshot.startTime;
      snapshot['endTime'] = fullSnapshot.endTime;
      snapshot['error'] = fullSnapshot.error;

      // Plan C: Include execution records
      if (fullSnapshot.errorRecords) {
        snapshot['errorRecords'] = fullSnapshot.errorRecords;
      }
      if (fullSnapshot.interruptionRecords) {
        snapshot['interruptionRecords'] = fullSnapshot.interruptionRecords;
      }
      if (fullSnapshot.eventRecords) {
        snapshot['eventRecords'] = fullSnapshot.eventRecords;
      }
    }

    // Include tool calls by default, unless explicitly disabled
    if (contentConfig?.includeToolCalls !== false) {
      if (fullSnapshot.iterationHistory) {
        snapshot['iterationHistory'] = fullSnapshot.iterationHistory;
      }
      if (fullSnapshot.currentIterationRecord) {
        snapshot['currentIterationRecord'] = fullSnapshot.currentIterationRecord;
      }
    }

    // Include streaming state fields
    if (fullSnapshot.isStreaming) {
      snapshot['isStreaming'] = fullSnapshot.isStreaming;
    }
    if (fullSnapshot.streamMessage) {
      snapshot['streamMessage'] = fullSnapshot.streamMessage;
    }
    if (fullSnapshot.pendingToolCallIds) {
      snapshot['pendingToolCallIds'] = fullSnapshot.pendingToolCallIds;
    }

    // Include trigger state by default for tracking trigger fires and limits
    if (contentConfig?.includeState !== false) {
      snapshot['triggerState'] = entity.exportTriggerState();
    }

    const hierarchyMetadata = entity.getHierarchyMetadata();
    if (hierarchyMetadata && hierarchyMetadata.children.length > 0) {
      snapshot['hierarchy'] = hierarchyMetadata;
    }

    return snapshot as unknown as AgentLoopStateSnapshot;
  }

  /**
   * Build checkpoint object
   */
  protected async buildCheckpoint(
    entity: AgentLoopEntity,
    currentState: AgentLoopStateSnapshot,
    checkpointType: TCheckpointType,
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    dependencies: CheckpointDependencies,
    metadata?: CheckpointMetadata,
    chainPosition?: number,
  ): Promise<AgentLoopCheckpoint> {
    const { getCheckpoint } = dependencies;

    if (checkpointType === "FULL") {
      return {
        id: checkpointId,
        agentLoopId: entity.id,
        timestamp,
        type: "FULL",
        snapshot: currentState,
        metadata: {
          ...metadata,
          customFields: {
            ...(metadata?.customFields || {}),
            chainPosition: 0,
          },
        },
      };
    }

    // Creating incremental checkpoints
    return this.buildDeltaCheckpoint(
      entity,
      currentState,
      checkpointId,
      timestamp,
      previousCheckpointIds,
      getCheckpoint,
      metadata,
      chainPosition,
    );
  }

  /**
   * Extract parent ID from checkpoint
   */
  protected extractParentId(checkpoint: AgentLoopCheckpoint): string {
    return checkpoint.agentLoopId;
  }

  /**
   * Create entity from restored state snapshot
   *
   * Uses the stored `restoreConfig` to provide AgentLoopRuntimeConfig.
   * Config must be set before calling restoreFromCheckpoint() via constructor or setConfig().
   *
   * @param parentId Parent entity ID (used as entity id)
   * @param snapshot Restored state snapshot
   * @returns Reconstructed AgentLoopEntity with config injected
   * @throws Error if restoreConfig is not set
   */
  protected createEntityFromSnapshot(
    parentId: string,
    snapshot: AgentLoopStateSnapshot,
  ): AgentLoopEntity {
    if (!this.restoreConfig) {
      throw new Error(
        "AgentLoopRuntimeConfig is required for restore. " +
          "Set it via constructor: new AgentLoopCheckpointCoordinator(config) " +
          "or via setConfig(config) before calling restoreFromCheckpoint().",
      );
    }
    return AgentLoopEntity.fromSnapshot(parentId, snapshot, this.restoreConfig);
  }

  /**
   * Build delta checkpoint
   */
  private async buildDeltaCheckpoint(
    entity: AgentLoopEntity,
    currentState: AgentLoopStateSnapshot,
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>,
    metadata?: CheckpointMetadata,
    chainPosition?: number,
  ): Promise<AgentLoopCheckpoint> {
    const previousCheckpointId = previousCheckpointIds[previousCheckpointIds.length - 1]!;
    const previousCheckpoint = await getCheckpoint(previousCheckpointId);

    if (!previousCheckpoint) {
      return {
        id: checkpointId,
        agentLoopId: entity.id,
        timestamp,
        type: "FULL" as const,
        snapshot: currentState,
        metadata: {
          ...metadata,
          customFields: {
            ...(metadata?.customFields || {}),
            chainPosition: 0,
          },
        },
      };
    }

    // Find baseline checkpoints (full checkpoints with snapshot)
    const baseCheckpoint = await this.findBaseCheckpoint(previousCheckpoint, getCheckpoint);

    // If a checkpoint containing a snapshot is still not found, downgrade to the full checkpoint
    if (!baseCheckpoint?.snapshot) {
      return {
        id: checkpointId,
        agentLoopId: entity.id,
        timestamp,
        type: "FULL" as const,
        snapshot: currentState,
        metadata: {
          ...metadata,
          customFields: {
            ...(metadata?.customFields || {}),
            chainPosition: 0,
          },
        },
      };
    }

    // Calculate the difference using inherited diffCalculator
    const delta = this.diffCalculator.calculateDelta(
      baseCheckpoint.snapshot as unknown as Record<string, unknown>,
      currentState as unknown as Record<string, unknown>,
    );

    // Find the baseline checkpoint ID
    const baseCheckpointId =
      previousCheckpoint.type === "FULL"
        ? previousCheckpoint.id
        : previousCheckpoint.baseCheckpointId!;

    return {
      id: checkpointId,
      agentLoopId: entity.id,
      timestamp,
      type: "DELTA",
      baseCheckpointId,
      previousCheckpointId,
      delta,
      metadata: {
        ...metadata,
        customFields: {
          ...(metadata?.customFields || {}),
          chainPosition: chainPosition ?? 1,
        },
      },
    };
  }

  /**
   * Find base checkpoint for delta calculation
   */
  private async findBaseCheckpoint(
    previousCheckpoint: AgentLoopCheckpoint,
    getCheckpoint: (id: string) => Promise<AgentLoopCheckpoint | null>,
  ): Promise<AgentLoopCheckpoint | null> {
    if (previousCheckpoint.type === "FULL") {
      return previousCheckpoint;
    }

    // If the previous checkpoint is delta, the nearest complete checkpoint needs to be found
    if (previousCheckpoint.baseCheckpointId) {
      const base = await getCheckpoint(previousCheckpoint.baseCheckpointId);
      if (base && base.snapshot) {
        return base;
      }
    }

    return null;
  }

  /**
   * Verify checkpoint integrity and compatibility
   */
  protected override validateCheckpoint(checkpoint: AgentLoopCheckpoint): void {
    // Call parent validation first
    super.validateCheckpoint(checkpoint);

    // Additional agent-specific validation
    if (!checkpoint.agentLoopId) {
      throw new AgentCheckpointError(
        "Invalid checkpoint: missing agentLoopId",
        "validate",
        checkpoint.id,
        checkpoint.agentLoopId,
      );
    }

    // Validation against checkpoint type
    if (checkpoint.type === "DELTA") {
      const deltaCheckpoint = checkpoint as DeltaCheckpoint<AgentLoopDelta>;
      if (!deltaCheckpoint.delta && !deltaCheckpoint.previousCheckpointId) {
        throw new AgentCheckpointError(
          "Invalid delta checkpoint: missing delta data and previous checkpoint reference",
          "validate",
          checkpoint.id,
          checkpoint.agentLoopId,
        );
      }
    } else {
      const fullCheckpoint = checkpoint as FullCheckpoint<AgentLoopStateSnapshot>;
      if (!fullCheckpoint.snapshot) {
        throw new AgentCheckpointError(
          "Invalid full checkpoint: missing state snapshot",
          "validate",
          checkpoint.id,
          checkpoint.agentLoopId,
        );
      }

      // Verify the snapshot structure
      const { snapshot } = fullCheckpoint;
      if (!snapshot.status) {
        throw new AgentCheckpointError(
          "Invalid checkpoint: incomplete state snapshot",
          "validate",
          checkpoint.id,
          checkpoint.agentLoopId,
        );
      }
    }
  }

  /**
   * Get version manager for compatibility checks and migrations
   */
  getVersionManager(): CheckpointVersionManager {
    return this.versionManager;
  }

  /**
   * Check if checkpoint needs version migration
   */
  needsVersionMigration(checkpoint: AgentLoopCheckpoint): boolean {
    const formatVersion = (checkpoint.metadata?.customFields?.["formatVersion"] as CheckpointFormatVersion) || CURRENT_CHECKPOINT_FORMAT_VERSION;
    const compatibility = this.versionManager.checkCompatibility(formatVersion);
    return compatibility.requiresMigration;
  }

  /**
   * Set checkpoint error handling configuration
   *
   * Enable custom error handling strategy for checkpoint operations.
   * Supports multiple strategies: silent, warn, strict, callback.
   *
   * @param errorStrategy Error handling strategy
   * @param onError Optional callback for "callback" strategy
   */
  setCheckpointErrorHandling(
    errorStrategy: CheckpointErrorStrategy,
    onError?: (error: Error, context: CheckpointErrorContext) => void | Promise<void>,
  ): void {
    this.checkpointErrorStrategy = errorStrategy;
    this.checkpointErrorHandler = new CheckpointErrorHandler(
      { strategy: errorStrategy, onError },
      logger,
    );

    logger.debug("Agent checkpoint error handling configured", {
      strategy: errorStrategy,
      hasCallback: !!onError,
    });
  }

  /**
   * Set checkpoint error handling strategy at runtime
   * @param strategy Error handling strategy
   */
  setCheckpointErrorStrategy(strategy: CheckpointErrorStrategy): void {
    this.checkpointErrorStrategy = strategy;

    if (!this.checkpointErrorHandler) {
      this.checkpointErrorHandler = new CheckpointErrorHandler(
        { strategy },
        logger,
      );
    } else {
      this.checkpointErrorHandler.setStrategy(strategy);
    }

    logger.debug("Agent checkpoint error strategy updated", { strategy });
  }

  /**
   * Get current checkpoint error handling strategy
   */
  getCheckpointErrorStrategy(): CheckpointErrorStrategy {
    return this.checkpointErrorStrategy;
  }

  /**
   * Get checkpoint error handler (for advanced usage)
   */
  getCheckpointErrorHandler(): CheckpointErrorHandler | undefined {
    return this.checkpointErrorHandler;
  }

  /**
   * Handle file checkpoint error using configured error handler or fallback to warn
   */
  private async handleFileCheckpointError(
    error: Error,
    operation: "create" | "restore",
    entityId: string,
    checkpointId?: string,
  ): Promise<void> {
    if (this.checkpointErrorHandler) {
      const result = await this.checkpointErrorHandler.handleError(error, {
        operation: "create",
        checkpointId: checkpointId || "unknown",
        entityId,
        triggerEvent: `file_checkpoint_${operation}`,
        timestamp: Date.now(),
      });
      if (result.shouldRethrow) {
        throw error;
      }
      return;
    }

    const operationText = operation === "create" ? "creation" : "restore";
    logger.warn(`File checkpoint ${operationText} failed (non-fatal, agent checkpoint saved)`, {
      agentLoopId: entityId,
      error: error.message,
    });
  }
}
