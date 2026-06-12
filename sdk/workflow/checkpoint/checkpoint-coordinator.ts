/**
 * Workflow Checkpoint Coordinator
 *
 * Coordinates the entire checkpoint lifecycle using template method pattern.
 * Extends BaseCheckpointCoordinator to eliminate code duplication.
 *
 * Design Philosophy:
 * - TEntity = WorkflowExecutionEntity
 * - TCheckpoint = Checkpoint
 * - TState = WorkflowExecutionStateSnapshot
 * - Uses template method for createCheckpoint flow
 * - Has separate restoreWorkflowFromCheckpoint for workflow-specific restoration
 */

import {
  WorkflowExecutionNotFoundError,
  CheckpointNotFoundError,
  WorkflowNotFoundError,
  WorkflowCheckpointError,
} from "@wf-agent/types";
import type { WorkflowExecution } from "@wf-agent/types";
import type {
  Checkpoint,
  CheckpointMetadata,
  WorkflowExecutionStateSnapshot,
  MessageMarkMap,
  DeltaStorageConfig,
  LLMMessage,
  NodeExecutionResult,
  TriggerRuntimeState,
  FullCheckpoint,
  DeltaCheckpoint,
  CheckpointDelta,
} from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../stores/workflow-execution-registry.js";
import type { WorkflowRegistry } from "../stores/workflow-registry.js";
import type { WorkflowGraphRegistry } from "../stores/workflow-graph-registry.js";
import type { JoinNodeConfig } from "@wf-agent/types";
import { CheckpointState } from "./checkpoint-state-manager.js";
import { ConversationSession } from "../../core/messaging/conversation-session.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { WorkflowExecutionEntity } from "../entities/workflow-execution-entity.js";
import { ExecutionState } from "../state-managers/execution-state.js";
import { WorkflowStateCoordinator } from "../state-managers/workflow-state-coordinator.js";
import { BaseDeltaRestorer } from "../../core/checkpoint/base-delta-restorer.js";
import { mergeMetadata } from "../../utils/index.js";
import { BaseCheckpointCoordinator } from "../../core/checkpoint/base-checkpoint-coordinator.js";
import type { CheckpointDependencies as BaseCheckpointDependencies } from "../../core/checkpoint/types.js";
import type { ExecutionHierarchyRegistry } from "../../core/registry/execution-hierarchy-registry.js";
import type { FileCheckpointManager } from "@wf-agent/common-utils";
import { HierarchyIntegrityService } from "../../core/execution/hierarchy-integrity-service.js";

const logger = createContextualLogger({ component: "CheckpointCoordinator" });

/**
 * Workflow-specific checkpoint dependencies
 * Extends base dependencies with workflow-specific fields
 */
export interface WorkflowCheckpointDependencies {
  workflowExecutionRegistry: WorkflowExecutionRegistry;
  checkpointStateManager: CheckpointState;
  workflowRegistry: WorkflowRegistry;
  workflowGraphRegistry: WorkflowGraphRegistry;
  hierarchyRegistry?: ExecutionHierarchyRegistry;
  deltaConfig?: DeltaStorageConfig;
  stateCoordinatorMap?: Map<string, WorkflowStateCoordinator>;
  fileCheckpointManager?: FileCheckpointManager;
}

/**
 * Backward compatibility alias
 */
export type CheckpointDependencies = WorkflowCheckpointDependencies;

/**
 * Checkpoint creation options
 */
export interface CheckpointOptions {
  metadata?: CheckpointMetadata;
  description?: string;
  nodeId?: string;
  toolId?: string;
  forceType?: "FULL" | "DELTA";
  skipIf?: "never" | "if_no_changes" | "if_recent";
  tags?: string[];
}

/**
 * Checkpoint creation options for convenience functions
 */
export interface CreateCheckpointOptions extends CheckpointOptions {
  workflowExecutionId: string;
}

/**
 * Restore context for storing intermediate results during restoration
 */
interface RestoreContext {
  conversationManager: ConversationSession;
  stateCoordinator: WorkflowStateCoordinator;
  checkpoint: Checkpoint;
}

/**
 * Workflow Checkpoint Coordinator
 *
 * Extends BaseCheckpointCoordinator to use template method pattern.
 * - createCheckpoint uses the base class template method
 * - restoreWorkflowFromCheckpoint handles workflow-specific 20-step restore
 */
export class CheckpointCoordinator extends BaseCheckpointCoordinator<
  Checkpoint,
  WorkflowExecutionEntity,
  WorkflowExecutionStateSnapshot
> {
  private currentDeps?: WorkflowCheckpointDependencies;
  private currentConversationManager?: ConversationSession;
  private restoreContext?: RestoreContext;

  // ============================================================================
  // Public Instance Methods - Workflow-specific entry points
  // ============================================================================

  /**
   * Create a workflow checkpoint
   * @param entity Workflow execution entity
   * @param dependencies Workflow checkpoint dependencies
   * @param options Checkpoint options
   * @param conversationManager Optional conversation manager
   * @returns Checkpoint ID
   */
  async createWorkflowCheckpoint(
    entity: WorkflowExecutionEntity,
    dependencies: WorkflowCheckpointDependencies,
    options?: CheckpointOptions,
    conversationManager?: ConversationSession,
  ): Promise<string> {
    this.currentDeps = dependencies;
    this.currentConversationManager =
      conversationManager ??
      dependencies.stateCoordinatorMap?.get(entity.id)?.getConversationManager();

    const metadata = this.buildMetadata(options);
    const checkpointId = await super.createCheckpoint(
      entity,
      this.toBaseDeps(dependencies),
      metadata,
    );

    // Create file checkpoint if manager is available
    if (dependencies.fileCheckpointManager) {
      try {
        await dependencies.fileCheckpointManager.createCheckpoint(entity.id);
        logger.info("File checkpoint created alongside execution checkpoint", {
          executionId: entity.id,
        });
      } catch (error) {
        logger.error("File checkpoint creation failed (non-fatal)", { error });
      }
    }

    return checkpointId;
  }

  /**
   * Restore workflow from checkpoint (workflow-specific restoration)
   *
   * This method handles the complete 20-step restore process:
   * 1-4: Load, validate, restore delta chain, get graph
   * 5-7: Create entity and restore state
   * 8-14: Create conversation, restore messages, create coordinator, restore triggers/context
   * 15-20: Restore operations, infer fork/join, validate hierarchy, register, file checkpoint
   *
   * @param checkpointId Checkpoint ID
   * @param dependencies Workflow checkpoint dependencies
   * @returns Restoration result with entity, state coordinator, and conversation manager
   */
  async restoreWorkflowFromCheckpoint(
    checkpointId: string,
    dependencies: WorkflowCheckpointDependencies,
  ): Promise<{
    workflowExecutionEntity: WorkflowExecutionEntity;
    stateCoordinator: WorkflowStateCoordinator;
    conversationManager: ConversationSession;
  }> {
    this.currentDeps = dependencies;

    // Step 1: Load checkpoint
    const checkpoint = await dependencies.checkpointStateManager.get(checkpointId);
    if (!checkpoint) {
      throw new CheckpointNotFoundError(`Checkpoint not found`, checkpointId);
    }

    // Step 2: Validate checkpoint
    this.validateCheckpoint(checkpoint);

    // Steps 3-4: Restore full state (handles delta chains)
    let workflowExecutionState: WorkflowExecutionStateSnapshot;
    if (checkpoint.type === "DELTA") {
      const restorer = new BaseDeltaRestorer<Checkpoint, WorkflowExecutionStateSnapshot>(id =>
        dependencies.checkpointStateManager.get(id),
      );
      const restoreResult = await restorer.restore(checkpointId);
      workflowExecutionState = restoreResult.snapshot;
    } else {
      const fullCp = checkpoint as FullCheckpoint<WorkflowExecutionStateSnapshot>;
      workflowExecutionState = fullCp.snapshot;
    }

    // Step 5: Get WorkflowGraph
    const processedWorkflow = dependencies.workflowGraphRegistry.get(checkpoint.workflowId);
    if (!processedWorkflow) {
      throw new WorkflowNotFoundError(`Processed workflow not found`, checkpoint.workflowId);
    }
    const graph = processedWorkflow;

    // Steps 6-7: Create entity and restore state
    const nodeResultsArray = Object.values(workflowExecutionState.nodeResults || {});
    const workflowExecution: Partial<WorkflowExecution> = {
      id: checkpoint.executionId,
      workflowId: checkpoint.workflowId,
      workflowVersion: "1.0.0",
      currentNodeId: workflowExecutionState.currentNodeId,
      input: workflowExecutionState.input,
      output: workflowExecutionState.output,
      nodeResults: nodeResultsArray,
      errors: workflowExecutionState.errors,
      forkJoinContext: workflowExecutionState.forkJoinContext,
      triggeredSubworkflowContext: workflowExecutionState.triggeredSubworkflowContext,
      graph,
    };

    const executionState = new ExecutionState();
    const entity = new WorkflowExecutionEntity(
      workflowExecution as WorkflowExecution,
      executionState,
    );

    // Step 7: Restore variables
    const variablesMap = new Map();
    const variableState = workflowExecutionState.variableState;
    if (variableState.variables) {
      for (const [name, value] of Object.entries(variableState.variables)) {
        variablesMap.set(name, {
          definition: { name, type: typeof value, value, readonly: false },
          value,
        });
      }
    }
    entity.variableStateManager.restoreFromSnapshot({ variables: variablesMap });

    // Step 8: Create ConversationSession
    const conversationManager = new ConversationSession();

    // Step 9: Restore messages
    if (
      workflowExecutionState.conversationState &&
      workflowExecutionState.conversationState.messages
    ) {
      conversationManager.addMessages(
        ...(workflowExecutionState.conversationState.messages as LLMMessage[]),
      );
    }

    // Step 10: Restore mark map and token usage
    if (workflowExecutionState.conversationState) {
      if (workflowExecutionState.conversationState.markMap) {
        conversationManager.setMarkMap(
          workflowExecutionState.conversationState.markMap as MessageMarkMap,
        );
      }
      conversationManager.setTokenUsageState(
        workflowExecutionState.conversationState.tokenUsage,
        workflowExecutionState.conversationState.currentRequestUsage,
      );
    }

    // Step 11: Create WorkflowStateCoordinator
    const stateCoordinator = new WorkflowStateCoordinator({
      conversationManager,
    });

    // Store context for post-restore
    this.restoreContext = {
      conversationManager,
      stateCoordinator,
      checkpoint,
    };

    // Steps 12-20: Post-restore operations
    await this.postRestore(entity, dependencies);

    return {
      workflowExecutionEntity: entity,
      stateCoordinator,
      conversationManager,
    };
  }

  // ============================================================================
  // Abstract Method Implementations
  // ============================================================================

  /**
   * Extract state from entity for checkpoint creation
   */
  protected extractState(entity: WorkflowExecutionEntity): WorkflowExecutionStateSnapshot {
    const workflowExecution = entity.getExecution();
    const convManager = this.currentConversationManager;

    // Create variable snapshot
    const vmSnapshot = entity.variableStateManager.createSnapshot();
    const variablesArray = Array.from(vmSnapshot.variables.values()).map(entry => entry.definition);

    // Convert nodeResults array to Record format
    const nodeResultsRecord: Record<string, NodeExecutionResult> = {};
    for (const result of workflowExecution.nodeResults) {
      nodeResultsRecord[result.nodeId] = result;
    }

    // Extract conversation state
    const conversationState = convManager
      ? {
          messages: convManager.getAllMessages(),
          markMap: convManager.getMarkMap(),
          tokenUsage: convManager.getTokenUsage(),
          currentRequestUsage: convManager.getCurrentRequestUsage(),
        }
      : {
          messages: [] as LLMMessage[],
          markMap: {
            currentBatch: 0,
            batchBoundaries: [0],
            originalIndices: [],
            boundaryToBatch: [],
          } as MessageMarkMap,
          tokenUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
          currentRequestUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
        };

    // Get trigger state snapshot
    const triggerStateSnapshot = entity.getTriggerStateSnapshot();

    // Get operation state snapshot
    const operationState = entity.state.getOperationStateSnapshot();

    // Build variable state
    const variableState: import("@wf-agent/types").CheckpointVariableState = {
      variables: Object.fromEntries(vmSnapshot.variables.entries()),
    };

    return {
      status: entity.getStatus(),
      currentNodeId: workflowExecution.currentNodeId,
      variables: variablesArray,
      variableState,
      input: workflowExecution.input,
      output: workflowExecution.output,
      nodeResults: nodeResultsRecord,
      errors: workflowExecution.errors,
      conversationState,
      triggerStates:
        triggerStateSnapshot.triggers.length > 0
          ? (new Map(Object.entries(triggerStateSnapshot.triggers)) as Map<
              string,
              TriggerRuntimeState
            >)
          : undefined,
      forkJoinContext: workflowExecution.forkJoinContext,
      triggeredSubworkflowContext: workflowExecution.triggeredSubworkflowContext,
      currentOperation: operationState ?? undefined,
    };
  }

  /**
   * Build checkpoint object
   */
  protected async buildCheckpoint(
    entity: WorkflowExecutionEntity,
    currentState: WorkflowExecutionStateSnapshot,
    checkpointType: "FULL" | "DELTA",
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    dependencies: BaseCheckpointDependencies<Checkpoint>,
    metadata?: CheckpointMetadata,
  ): Promise<Checkpoint> {
    const { getCheckpoint } = dependencies;

    if (checkpointType === "FULL") {
      return {
        id: checkpointId,
        executionId: entity.id,
        workflowId: entity.getWorkflowId(),
        timestamp,
        type: "FULL",
        snapshot: currentState,
        metadata,
      };
    }

    // Delta checkpoint
    const previousCheckpointId = previousCheckpointIds[0]!;
    const previousCheckpoint = await getCheckpoint(previousCheckpointId);

    if (!previousCheckpoint) {
      // Downgrade to full checkpoint
      return {
        id: checkpointId,
        executionId: entity.id,
        workflowId: entity.getWorkflowId(),
        timestamp,
        type: "FULL",
        snapshot: currentState,
        metadata,
      };
    }

    // Get previous state
    let previousState: WorkflowExecutionStateSnapshot;
    if (previousCheckpoint.type === "DELTA") {
      const restorer = new BaseDeltaRestorer<Checkpoint, WorkflowExecutionStateSnapshot>(
        getCheckpoint,
      );
      const restoreResult = await restorer.restore(previousCheckpointId);
      previousState = restoreResult.snapshot;
    } else {
      const fullCp = previousCheckpoint as FullCheckpoint<WorkflowExecutionStateSnapshot>;
      previousState = fullCp.snapshot;
    }

    // Calculate delta
    const delta = this.diffCalculator.calculateDelta(
      previousState as unknown as Record<string, unknown>,
      currentState as unknown as Record<string, unknown>,
    );

    // Find base checkpoint ID
    let baseCheckpointId: string;
    if (previousCheckpoint.type === "FULL") {
      baseCheckpointId = previousCheckpoint.id;
    } else {
      const deltaCp = previousCheckpoint as DeltaCheckpoint<CheckpointDelta>;
      baseCheckpointId = deltaCp.baseCheckpointId;
    }

    return {
      id: checkpointId,
      executionId: entity.id,
      workflowId: entity.getWorkflowId(),
      timestamp,
      type: "DELTA",
      baseCheckpointId,
      previousCheckpointId,
      delta,
      metadata,
    };
  }

  /**
   * Extract parent ID from checkpoint
   */
  protected extractParentId(checkpoint: Checkpoint): string {
    return checkpoint.executionId;
  }

  /**
   * Create entity from restored state snapshot
   * Note: Extra parameters (checkpoint, dependencies) are stored as instance variables
   * before this method is called, and accessed via this.restoreContext/this.currentDeps.
   */
  protected createEntityFromSnapshot(
    _parentId: string,
    snapshot: WorkflowExecutionStateSnapshot,
  ): WorkflowExecutionEntity {
    // This method is called by the base class restoreFromCheckpoint template.
    // However, for workflow-specific restoration, we use restoreWorkflowFromCheckpoint
    // which handles the full 20-step process. This method is kept for interface compliance
    // but delegates to the same logic when called via the base class.

    const deps = this.currentDeps!;
    const checkpoint = this.restoreContext?.checkpoint!;

    return this.buildEntityFromSnapshot(_parentId, snapshot, checkpoint, deps);
  }

  /**
   * Actual entity construction logic
   */
  private buildEntityFromSnapshot(
    _parentId: string,
    snapshot: WorkflowExecutionStateSnapshot,
    checkpoint: Checkpoint,
    dependencies: WorkflowCheckpointDependencies,
  ): WorkflowExecutionEntity {
    const processedWorkflow = dependencies.workflowGraphRegistry.get(checkpoint.workflowId);
    if (!processedWorkflow) {
      throw new WorkflowNotFoundError(`Processed workflow not found`, checkpoint.workflowId);
    }
    const graph = processedWorkflow;

    const nodeResultsArray = Object.values(snapshot.nodeResults || {});
    const workflowExecution: Partial<WorkflowExecution> = {
      id: checkpoint.executionId,
      workflowId: checkpoint.workflowId,
      workflowVersion: "1.0.0",
      currentNodeId: snapshot.currentNodeId,
      input: snapshot.input,
      output: snapshot.output,
      nodeResults: nodeResultsArray,
      errors: snapshot.errors,
      forkJoinContext: snapshot.forkJoinContext,
      triggeredSubworkflowContext: snapshot.triggeredSubworkflowContext,
      graph,
    };

    const executionState = new ExecutionState();
    const entity = new WorkflowExecutionEntity(
      workflowExecution as WorkflowExecution,
      executionState,
    );

    // Restore variables
    const variablesMap = new Map();
    const variableState = snapshot.variableState;
    if (variableState.variables) {
      for (const [name, value] of Object.entries(variableState.variables)) {
        variablesMap.set(name, {
          definition: { name, type: typeof value, value, readonly: false },
          value,
        });
      }
    }
    entity.variableStateManager.restoreFromSnapshot({ variables: variablesMap });

    // Conversation session and state coordinator are created in restoreWorkflowFromCheckpoint
    // This method is only called when using the base class template (not the workflow-specific path)
    return entity;
  }

  // ============================================================================
  // Post-Restore Hook
  // ============================================================================

  /**
   * Post-restore operations (steps 15-20 of the 20-step restore process)
   * - Step 15: Restore operation state
   * - Step 16: Infer FORK/JOIN completion
   * - Step 17: Validate hierarchy
   * - Step 18: Reestablish parent-child relationships
   * - Step 19: Register with registry
   * - Step 20: File checkpoint restoration
   */
  private async postRestore(
    entity: WorkflowExecutionEntity,
    dependencies: WorkflowCheckpointDependencies,
  ): Promise<void> {
    const ctx = this.restoreContext!;

    // Step 15: Restore operation state (already done in buildEntityFromSnapshot for basic case)
    // For workflow-specific path, this is handled in restoreWorkflowFromCheckpoint

    // Step 16: Infer FORK/JOIN completion status (if current node is JOIN)
    if (entity.getGraph()) {
      const currentNode = entity.getGraph().getNode(entity.getCurrentNodeId());
      if (currentNode && currentNode.type === "JOIN") {
        const joinStatus = await this._inferForkJoinState(
          entity.getCurrentNodeId(),
          entity,
          dependencies.workflowExecutionRegistry,
        );

        logger.info("Inferred JOIN completion status", {
          executionId: entity.id,
          joinNodeId: entity.getCurrentNodeId(),
          completedPaths: Array.from(joinStatus.completedPaths),
          pendingPaths: Array.from(joinStatus.pendingPaths),
          failedPaths: Array.from(joinStatus.failedPaths),
        });
      }
    }

    // Step 17: Validate hierarchy integrity
    const { hierarchyRegistry } = dependencies;
    if (hierarchyRegistry) {
      const hierarchyMetadata = entity.getHierarchyMetadata();
      if (hierarchyMetadata) {
        const validation = HierarchyIntegrityService.validateIntegrity(
          hierarchyMetadata,
          hierarchyRegistry,
        );

        if (!validation.valid) {
          logger.warn("Hierarchy integrity issues detected after checkpoint restore", {
            executionId: entity.id,
            issues: validation.issues,
          });
        }
      }
    }

    // Step 18: Reestablish parent-child relationship for child workflows
    const childExecutionIds = ((ctx.checkpoint.metadata?.customFields as Record<string, unknown>) ||
      {})["childExecutionIds"];
    if (childExecutionIds && Array.isArray(childExecutionIds)) {
      for (const childWorkflowExecutionId of childExecutionIds as string[]) {
        const childCheckpointId = await this._findChildCheckpoint(
          childWorkflowExecutionId,
          dependencies.checkpointStateManager,
        );
        if (childCheckpointId) {
          const childResult = await this.restoreWorkflowFromCheckpoint(
            childCheckpointId,
            dependencies,
          );

          childResult.workflowExecutionEntity.setParentContext({
            parentType: "WORKFLOW",
            parentId: entity.id,
          });

          dependencies.workflowExecutionRegistry.register(childResult.workflowExecutionEntity);
          dependencies.workflowExecutionRegistry.registerStateCoordinator(
            childResult.workflowExecutionEntity.id,
            childResult.stateCoordinator,
          );

          entity.registerChild({
            childType: "WORKFLOW",
            childId: childWorkflowExecutionId,
            createdAt: Date.now(),
          });
        }
      }
    }

    // Step 19: Register with registry
    dependencies.workflowExecutionRegistry.register(entity);
    dependencies.workflowExecutionRegistry.registerStateCoordinator(
      entity.id,
      ctx.stateCoordinator,
    );

    // Step 20: Restore file checkpoint
    if (dependencies.fileCheckpointManager) {
      try {
        const fileCheckpoints = await dependencies.fileCheckpointManager
          .getStorage()
          .listByEntity(ctx.checkpoint.executionId, { limit: 1 });
        if (fileCheckpoints.length > 0) {
          const result = await dependencies.fileCheckpointManager.restoreCheckpoint(
            ctx.checkpoint.executionId,
            fileCheckpoints[0]!.id,
          );
          logger.info("File checkpoint restored alongside execution checkpoint", {
            executionId: ctx.checkpoint.executionId,
            restoredCount: result.restoredCount,
            deletedCount: result.deletedCount,
            skippedCount: result.skippedCount,
          });
        }
      } catch (error) {
        logger.error("File checkpoint restore failed (non-fatal)", { error });
      }
    }
  }

  // ============================================================================
  // Static Aliases - Backward Compatibility
  // ============================================================================

  /**
   * Create a checkpoint (static alias for backward compatibility)
   */
  static async createCheckpoint(
    workflowExecutionId: string,
    dependencies: CheckpointDependencies,
    options?: CheckpointOptions,
    conversationManager?: ConversationSession,
  ): Promise<string> {
    const coordinator = new CheckpointCoordinator();
    const entity = dependencies.workflowExecutionRegistry.get(workflowExecutionId);
    if (!entity) {
      throw new WorkflowExecutionNotFoundError(
        `WorkflowExecutionEntity not found`,
        workflowExecutionId,
      );
    }
    return coordinator.createWorkflowCheckpoint(entity, dependencies, options, conversationManager);
  }

  /**
   * Restore from checkpoint (static alias for backward compatibility)
   * Delegates to restoreWorkflowFromCheckpoint internally
   */
  static async restoreFromCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies,
  ): Promise<{
    workflowExecutionEntity: WorkflowExecutionEntity;
    stateCoordinator: WorkflowStateCoordinator;
    conversationManager: ConversationSession;
  }> {
    const coordinator = new CheckpointCoordinator();
    return coordinator.restoreWorkflowFromCheckpoint(checkpointId, dependencies);
  }

  /**
   * Create node checkpoint (static alias)
   */
  static async createNodeCheckpoint(
    workflowExecutionId: string,
    nodeId: string,
    dependencies: CheckpointDependencies,
    options?: CheckpointOptions,
  ): Promise<string> {
    return this.createCheckpoint(workflowExecutionId, dependencies, {
      ...options,
      nodeId,
      description: options?.description || `Node checkpoint for node ${nodeId}`,
    });
  }

  /**
   * Create tool checkpoint (static alias)
   */
  static async createToolCheckpoint(
    workflowExecutionId: string,
    toolId: string,
    dependencies: CheckpointDependencies,
    options?: CheckpointOptions,
  ): Promise<string> {
    return this.createCheckpoint(workflowExecutionId, dependencies, {
      ...options,
      toolId,
      description: options?.description || `Tool checkpoint for tool ${toolId}`,
    });
  }

  /**
   * Create checkpoints in batches (static alias)
   */
  static async createCheckpoints(
    optionsList: CreateCheckpointOptions[],
    dependencies: CheckpointDependencies,
  ): Promise<string[]> {
    const promises = optionsList.map(options =>
      this.createCheckpoint(options.workflowExecutionId, dependencies, {
        metadata: options.metadata,
        description:
          options.description || `Checkpoint for execution ${options.workflowExecutionId}`,
        nodeId: options.nodeId,
        toolId: options.toolId,
        tags: options.tags,
        forceType: options.forceType,
        skipIf: options.skipIf,
      }),
    );
    return await Promise.all(promises);
  }

  // ============================================================================
  // Private Helpers
  // ============================================================================

  /**
   * Build metadata from options
   */
  private buildMetadata(options?: CheckpointOptions): CheckpointMetadata | undefined {
    if (!options) return undefined;

    return {
      ...options.metadata,
      description: options.description ?? options.metadata?.description,
      tags: options.tags ?? options.metadata?.tags,
      ...(options.nodeId
        ? {
            customFields: mergeMetadata(options.metadata?.customFields || {}, {
              nodeId: options.nodeId,
            }),
          }
        : {}),
      ...(options.toolId
        ? {
            customFields: mergeMetadata(options.metadata?.customFields || {}, {
              toolId: options.toolId,
            }),
          }
        : {}),
    } as CheckpointMetadata;
  }

  /**
   * Convert workflow dependencies to base dependencies
   */
  private toBaseDeps(deps: WorkflowCheckpointDependencies): BaseCheckpointDependencies<Checkpoint> {
    return {
      saveCheckpoint: checkpoint => deps.checkpointStateManager.create(checkpoint),
      getCheckpoint: id => deps.checkpointStateManager.get(id),
      listCheckpoints: parentId => deps.checkpointStateManager.list({ parentId }),
      deltaConfig: deps.deltaConfig,
    };
  }

  /**
   * Determine checkpoint type (preserves original workflow behavior)
   * Uses baselineInterval without maxDeltaChainLength
   */
  protected override determineCheckpointType(
    checkpointCount: number,
    config: DeltaStorageConfig,
  ): "FULL" | "DELTA" {
    if (!config.enabled) {
      return "FULL";
    }
    if (checkpointCount === 0) {
      return "FULL";
    }
    if (checkpointCount % config.baselineInterval === 0) {
      return "FULL";
    }
    return "DELTA";
  }

  /**
   * Infer FORK/JOIN completion status from JOIN node
   */
  private async _inferForkJoinState(
    joinNodeId: string,
    entity: WorkflowExecutionEntity,
    registry: WorkflowExecutionRegistry,
  ): Promise<{
    completedPaths: Set<string>;
    pendingPaths: Set<string>;
    failedPaths: Set<string>;
  }> {
    const graph = entity.getGraph();
    const joinNode = graph.getNode(joinNodeId);

    if (!joinNode || joinNode.type !== "JOIN") {
      return {
        completedPaths: new Set(),
        pendingPaths: new Set(),
        failedPaths: new Set(),
      };
    }

    const joinConfig = joinNode.originalNode?.config as JoinNodeConfig | undefined;
    const forkPathIds = joinConfig?.forkPathIds || [];

    if (forkPathIds.length === 0) {
      logger.warn("JOIN node has no forkPathIds configured", {
        joinNodeId,
        executionId: entity.id,
      });
      return {
        completedPaths: new Set(),
        pendingPaths: new Set(),
        failedPaths: new Set(),
      };
    }

    const allExecutions = registry.getAll();
    const parentExecutionId = entity.id;

    const childExecutions = Array.from(allExecutions.values()).filter(exec => {
      const execData = exec.getWorkflowExecutionData();
      return (
        execData.executionType === "FORK_JOIN" &&
        execData.hierarchy?.parent?.parentId === parentExecutionId
      );
    });

    const completedPaths = new Set<string>();
    const pendingPaths = new Set<string>();
    const failedPaths = new Set<string>();

    for (const pathId of forkPathIds) {
      const childExecution = childExecutions.find(
        exec => exec.getWorkflowExecutionData().forkJoinContext?.forkPathId === pathId,
      );

      if (!childExecution) {
        pendingPaths.add(pathId);
      } else {
        const status = childExecution.getStatus();

        if (status === "COMPLETED") {
          completedPaths.add(pathId);
        } else if (status === "FAILED" || status === "CANCELLED" || status === "TIMEOUT") {
          failedPaths.add(pathId);
        } else {
          pendingPaths.add(pathId);
        }
      }
    }

    return { completedPaths, pendingPaths, failedPaths };
  }

  /**
   * Find child checkpoint
   */
  private async _findChildCheckpoint(
    childWorkflowExecutionId: string,
    checkpointStateManager: CheckpointState,
  ): Promise<string | undefined> {
    const checkpointIds = await checkpointStateManager.list({
      parentId: childWorkflowExecutionId,
    });
    if (checkpointIds.length === 0) {
      return undefined;
    }
    return checkpointIds[0];
  }

  /**
   * Validate checkpoint integrity
   */
  protected override validateCheckpoint(checkpoint: Checkpoint): void {
    super.validateCheckpoint(checkpoint);

    const checkpointId = checkpoint.id;
    const workflowId = checkpoint.workflowId;
    const executionId = checkpoint.executionId;

    if (!workflowId) {
      throw new WorkflowCheckpointError(
        "Invalid checkpoint: missing workflowId",
        "validate",
        checkpointId,
        undefined,
        workflowId,
        executionId,
      );
    }

    if (!executionId) {
      throw new WorkflowCheckpointError(
        "Invalid checkpoint: missing executionId",
        "validate",
        checkpointId,
        undefined,
        workflowId,
        executionId,
      );
    }
  }
}
