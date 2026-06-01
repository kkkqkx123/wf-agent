/**
 * Checkpoint Coordinator
 * A stateless service that coordinates the entire checkpoint process
 */

import {
  WorkflowExecutionNotFoundError,
  CheckpointNotFoundError,
  WorkflowNotFoundError,
  WorkflowCheckpointError,
} from "@wf-agent/types";
import { DEFAULT_DELTA_STORAGE_CONFIG } from "../../core/checkpoint/utils/constants.js";
import type { WorkflowExecution } from "@wf-agent/types";
import type {
  Checkpoint,
  CheckpointMetadata,
  WorkflowExecutionStateSnapshot,
  MessageMarkMap,
  DeltaStorageConfig,
  TCheckpointType,
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
import { BaseDiffCalculator } from "../../core/checkpoint/base-diff-calculator.js";
import { BaseDeltaRestorer } from "../../core/checkpoint/base-delta-restorer.js";
import { generateId, mergeMetadata } from "../../utils/index.js";
import { now } from "@wf-agent/common-utils";
import type { Metadata } from "@wf-agent/types";
import type { ExecutionHierarchyRegistry } from "../../core/registry/execution-hierarchy-registry.js";
import type { FileCheckpointManager } from "@wf-agent/common-utils";
import { HierarchyIntegrityService } from "../../core/execution/hierarchy-integrity-service.js";

const logger = createContextualLogger({ component: "CheckpointCoordinator" });

/**
 * Checkpoint dependencies
 */
export interface CheckpointDependencies {
  workflowExecutionRegistry: WorkflowExecutionRegistry;
  checkpointStateManager: CheckpointState;
  workflowRegistry: WorkflowRegistry;
  workflowGraphRegistry: WorkflowGraphRegistry;
  /** Execution Hierarchy Registry for integrity validation */
  hierarchyRegistry?: ExecutionHierarchyRegistry;
  /** Incremental storage configuration (optional) */
  deltaConfig?: DeltaStorageConfig;
  /** WorkflowStateCoordinator map (optional, for new architecture) */
  stateCoordinatorMap?: Map<string, WorkflowStateCoordinator>;
  /** File checkpoint manager (optional, enables file state checkpointing) */
  fileCheckpointManager?: FileCheckpointManager;
}

/**
 * Checkpoint creation options
 * Note: Sync persistence is handled at the storage adapter layer, not here.
 */
export interface CheckpointOptions {
  /** Custom metadata for the checkpoint */
  metadata?: CheckpointMetadata;

  /** Human-readable description for the checkpoint */
  description?: string;

  /** Source node ID (marks which node triggered the checkpoint) */
  nodeId?: string;

  /** Source tool ID (marks which tool triggered the checkpoint) */
  toolId?: string;

  /**
   * Force checkpoint type
   * - Omitted: Coordinator auto-determines based on delta config
   * - 'FULL': Force full snapshot
   * - 'DELTA': Force delta (auto-downgrades to FULL if no baseline exists)
   */
  forceType?: 'FULL' | 'DELTA';

  /**
   * Skip condition
   * - 'never': Always create (default)
   * - 'if_no_changes': Skip if no changes since last checkpoint
   * - 'if_recent': Skip if a checkpoint was created recently
   */
  skipIf?: 'never' | 'if_no_changes' | 'if_recent';

  /** Tags for query/filtering */
  tags?: string[];
}

/**
 * Checkpoint creation options for the convenience function
 */
export interface CreateCheckpointOptions extends CheckpointOptions {
  /** WorkflowExecution ID */
  workflowExecutionId: string;
}

/**
 * Checkpoint Coordinator (completely stateless)
 * Uses base diff calculator and delta restorer to eliminate code duplication
 */
export class CheckpointCoordinator {
  private static diffCalculator = new BaseDiffCalculator();

  /**
   * Create a checkpoint (static method)
   * @param workflowExecutionId WorkflowExecution ID
   * @param dependencies Dependencies
   * @param options Checkpoint options (metadata, description, tags, etc.)
   * @param conversationManager Optional ConversationSession (if not using stateCoordinatorMap)
   * @returns Checkpoint ID
   */
  static async createCheckpoint(
    workflowExecutionId: string,
    dependencies: CheckpointDependencies,
    options?: CheckpointOptions,
    conversationManager?: ConversationSession,
  ): Promise<string> {
    const { workflowExecutionRegistry, checkpointStateManager, deltaConfig, stateCoordinatorMap } =
      dependencies;
    const config = { ...DEFAULT_DELTA_STORAGE_CONFIG, ...deltaConfig };

    // Step 1: Retrieve the WorkflowExecutionEntity object from the WorkflowExecutionRegistry
    const workflowExecutionEntity = workflowExecutionRegistry.get(workflowExecutionId);
    if (!workflowExecutionEntity) {
      throw new WorkflowExecutionNotFoundError(
        `WorkflowExecutionEntity not found`,
        workflowExecutionId,
      );
    }

    const workflowExecution = workflowExecutionEntity.getExecution();

    // Get ConversationSession from stateCoordinatorMap or parameter
    const convManager =
      conversationManager ??
      stateCoordinatorMap?.get(workflowExecutionId)?.getConversationManager();

    // Step 2: Extract WorkflowExecutionStateSnapshot
    const currentState = CheckpointCoordinator.extractWorkflowExecutionState(
      workflowExecutionEntity,
      workflowExecution,
      convManager,
    );

    // Step 3: Retrieve the previous checkpoint
    const previousCheckpointIds = await checkpointStateManager.list({
      parentId: workflowExecutionId,
    });
    const checkpointCount = previousCheckpointIds.length;

    // Step 4: Determine the type of checkpoint
    // If forceType is specified, use it directly; otherwise determine automatically
    const checkpointType = options?.forceType ?? CheckpointCoordinator.determineCheckpointType(checkpointCount, config);

    // Step 5: Generate a unique checkpointId and timestamp
    const checkpointId = generateId();
    const timestamp = now();

    // Build the final metadata from options
    const metadata = options?.metadata
      ? mergeMetadata(options.metadata as Metadata, {
          ...(options.description ? { description: options.description } : {}),
          ...(options.tags ? { tags: options.tags } : {}),
          ...(options.nodeId ? { customFields: mergeMetadata({} as Metadata, { nodeId: options.nodeId }) } : {}),
          ...(options.toolId ? { customFields: mergeMetadata({} as Metadata, { toolId: options.toolId }) } : {}),
        })
      : ({
          ...(options?.description ? { description: options.description } : {}),
          ...(options?.tags ? { tags: options.tags } : {}),
          ...(options?.nodeId ? { customFields: { nodeId: options.nodeId } } : {}),
          ...(options?.toolId ? { customFields: { toolId: options.toolId } } : {}),
        } as CheckpointMetadata | undefined);

    // Step 6: Create a checkpoint
    let checkpoint: Checkpoint;

    if (checkpointType === "FULL") {
      // Create a complete checkpoint
      checkpoint = {
        id: checkpointId,
        executionId: workflowExecutionEntity.id,
        workflowId: workflowExecutionEntity.getWorkflowId(),
        timestamp,
        type: "FULL",
        snapshot: currentState,
        metadata,
      };
    } else {
      // Create incremental checkpoints
      const previousCheckpointId = previousCheckpointIds[0]!;
      const previousCheckpoint = await checkpointStateManager.get(previousCheckpointId);

      if (!previousCheckpoint) {
        // If the previous checkpoint cannot be obtained, downgrade to the full checkpoint.
        checkpoint = {
          id: checkpointId,
          executionId: workflowExecutionEntity.id,
          workflowId: workflowExecutionEntity.getWorkflowId(),
          timestamp,
          type: "FULL",
          snapshot: currentState,
          metadata,
        };
      } else {
        // Get the complete status of the previous checkpoint.
        let previousState: WorkflowExecutionStateSnapshot;
        if (previousCheckpoint.type === "DELTA") {
          // If the previous checkpoint was an incremental checkpoint, the full state needs to be restored.
          const restorer = new BaseDeltaRestorer<Checkpoint, WorkflowExecutionStateSnapshot>(
            id => checkpointStateManager.get(id),
          );
          const restoreResult = await restorer.restore(previousCheckpointId);
          previousState = restoreResult.snapshot;
        } else {
          const fullCp = previousCheckpoint as FullCheckpoint<WorkflowExecutionStateSnapshot>;
          previousState = fullCp.snapshot;
        }

        // Calculate the difference
        const delta = CheckpointCoordinator.diffCalculator.calculateDelta(
          previousState as unknown as Record<string, unknown>,
          currentState as unknown as Record<string, unknown>,
        );

        // Find the baseline checkpoint ID
        let baseCheckpointId: string;
        if (previousCheckpoint.type === "FULL") {
          baseCheckpointId = previousCheckpoint.id;
        } else {
          const deltaCp = previousCheckpoint as DeltaCheckpoint<CheckpointDelta>;
          baseCheckpointId = deltaCp.baseCheckpointId;
        }

        checkpoint = {
          id: checkpointId,
          executionId: workflowExecutionEntity.id,
          workflowId: workflowExecutionEntity.getWorkflowId(),
          timestamp,
          type: "DELTA",
          baseCheckpointId,
          previousCheckpointId,
          delta,
          metadata,
        };
      }
    }

    // Step 7: Call CheckpointState to create a checkpoint
    const executionCheckpointId = await checkpointStateManager.create(checkpoint);

    // Step 8: Create file checkpoint (if file checkpoint manager is available)
    if (dependencies.fileCheckpointManager) {
      try {
        await dependencies.fileCheckpointManager.createCheckpoint(workflowExecutionId);
        logger.info("File checkpoint created alongside execution checkpoint", {
          executionId: workflowExecutionId,
        });
      } catch (error) {
        logger.error("File checkpoint creation failed (non-fatal)", { error });
      }
    }

    return executionCheckpointId;
  }

  /**
   * Extract a workflow execution state snapshot
   * @param workflowExecutionEntity Workflow execution entity
   * @param workflowExecution Workflow execution object
   * @param conversationManager Optional ConversationSession
   * @returns Workflow execution state snapshot
   */
  private static extractWorkflowExecutionState(
    workflowExecutionEntity: WorkflowExecutionEntity,
    workflowExecution: WorkflowExecution,
    conversationManager?: ConversationSession,
    ): WorkflowExecutionStateSnapshot {
    // Create a variable snapshot using VariableManager
    const vmSnapshot = workflowExecutionEntity.variableStateManager.createSnapshot();
    
    // Convert Map to array for checkpoint format
    const variablesArray = Array.from(vmSnapshot.variables.values()).map(entry => entry.definition);

    // Convert the `nodeResults` array into Record format.
    const nodeResultsRecord: Record<string, NodeExecutionResult> = {};
    for (const result of workflowExecution.nodeResults) {
      nodeResultsRecord[result.nodeId] = result;
    }

    // Save the complete message history and index status to the checkpoint.
    const conversationState = conversationManager
      ? {
          messages: conversationManager.getAllMessages(),
          markMap: conversationManager.getMarkMap(),
          tokenUsage: conversationManager.getTokenUsage(),
          currentRequestUsage: conversationManager.getCurrentRequestUsage(),
        }
      : {
          messages: [],
          markMap: {
            currentBatch: 0,
            batchBoundaries: [0],
            originalIndices: [],
            boundaryToBatch: [],
          },
          tokenUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
          currentRequestUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
        };

    // Get a snapshot of the trigger status.
    const triggerStateSnapshot = workflowExecutionEntity.getTriggerStateSnapshot();

    // Capture operation state for mid-node resume
    const operationState = workflowExecutionEntity.state.getOperationStateSnapshot();

    // Build CheckpointVariableState from VariableManager snapshot
    const variableState: import("@wf-agent/types").CheckpointVariableState = {
      variables: Object.fromEntries(vmSnapshot.variables.entries()),
    };

    return {
      status: workflowExecutionEntity.getStatus(),
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
   * Determine the checkpoint type
   * @param checkpointCount: The current number of checkpoints
   * @param config: Incremental storage configuration
   * @returns: The type of checkpoint
   */
  private static determineCheckpointType(
    checkpointCount: number,
    config: DeltaStorageConfig,
  ): TCheckpointType {
    // If incremental storage is not enabled, a complete checkpoint is always created.
    if (!config.enabled) {
      return "FULL";
    }

    // The first checkpoint must be a complete checkpoint.
    if (checkpointCount === 0) {
      return "FULL";
    }

    // Create a complete checkpoint every baselineInterval checkpoints.
    if (checkpointCount % config.baselineInterval === 0) {
      return "FULL";
    }

    // Create incremental checkpoints in other cases.
    return "DELTA";
  }

  /**
   * Restore the WorkflowExecutionEntity state from a checkpoint (static method)
   * @param checkpointId Checkpoint ID
   * @param dependencies Dependencies
   * @returns WorkflowExecutionBuildResult containing WorkflowExecutionEntity, WorkflowStateCoordinator, and ConversationSession
   */
  static async restoreFromCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies,
  ): Promise<{
    workflowExecutionEntity: WorkflowExecutionEntity;
    stateCoordinator: WorkflowStateCoordinator;
    conversationManager: ConversationSession;
  }> {
    const { workflowExecutionRegistry, checkpointStateManager, workflowGraphRegistry } =
      dependencies;

    // Step 1: Load the checkpoint from CheckpointState
    const checkpoint = await checkpointStateManager.get(checkpointId);
    if (!checkpoint) {
      throw new CheckpointNotFoundError(`Checkpoint not found`, checkpointId);
    }

    // Step 2: Verify the integrity and compatibility of the checkpoint
    CheckpointCoordinator.validateCheckpoint(checkpoint);

    // Step 3: Obtain the complete workflow execution state (processing incremental checkpoints)
    let workflowExecutionState: WorkflowExecutionStateSnapshot;
    if (checkpoint.type === "DELTA") {
      // If it's an incremental checkpoint, the full state needs to be restored.
      const restorer = new BaseDeltaRestorer<Checkpoint, WorkflowExecutionStateSnapshot>(
        id => checkpointStateManager.get(id),
      );
      const restoreResult = await restorer.restore(checkpointId);
      workflowExecutionState = restoreResult.snapshot;
    } else {
      // Full checkpoint, use it directly.
      const fullCp = checkpoint as FullCheckpoint<WorkflowExecutionStateSnapshot>;
      workflowExecutionState = fullCp.snapshot;
    }

    // Step 4: Get the WorkflowGraph from the WorkflowGraphRegistry
    // The WorkflowGraph contains the complete preprocessed graph structure
    const processedWorkflow = workflowGraphRegistry.get(checkpoint.workflowId);
    if (!processedWorkflow) {
      throw new WorkflowNotFoundError(`Processed workflow not found`, checkpoint.workflowId);
    }

    // The WorkflowGraph itself is a Graph, containing the complete graph structure.
    // Design purpose: The restored WorkflowExecution requires a complete graph structure
    // Continue to execute the workflow (for example: finding nodes, traversing edges, performing graph algorithms, etc.)
    const graph = processedWorkflow;

    // Step 5: Restore WorkflowExecution state
    // Convert nodeResults Record back to array format
    const nodeResultsArray = Object.values(workflowExecutionState.nodeResults || {});

    const workflowExecution: Partial<WorkflowExecution> = {
      id: checkpoint.executionId,
      workflowId: checkpoint.workflowId,
      workflowVersion: "1.0.0", // TODO: Retrieve the version from the checkpoint metadata
      currentNodeId: workflowExecutionState.currentNodeId,
      input: workflowExecutionState.input,
      output: workflowExecutionState.output,
      nodeResults: nodeResultsArray,
      errors: workflowExecutionState.errors,
      forkJoinContext: workflowExecutionState.forkJoinContext,
      triggeredSubworkflowContext: workflowExecutionState.triggeredSubworkflowContext,
      graph,
    };

    // Step 6: Create WorkflowExecutionEntity (without ConversationManager)
    const executionState = new ExecutionState();
    const workflowExecutionEntity = new WorkflowExecutionEntity(
      workflowExecution as WorkflowExecution,
      executionState,
    );

    // Step 7: Restore the variable snapshot using VariableManager
    // Convert from CheckpointVariableState back to VariableManager format
    const variablesMap = new Map();
    
    const variableState = workflowExecutionState.variableState;
    
    // Restore all variables from flat structure
    if (variableState.variables) {
      for (const [name, value] of Object.entries(variableState.variables)) {
        variablesMap.set(name, {
          definition: { name, type: typeof value, value, readonly: false },
          value,
        });
      }
    }
    
    workflowExecutionEntity.variableStateManager.restoreFromSnapshot({
      variables: variablesMap,
    });

    // Step 8: Create the ConversationSession
    const conversationManager = new ConversationSession();

    // Step 9: Restore the complete message history from the checkpoint snapshot
    if (
      workflowExecutionState.conversationState &&
      workflowExecutionState.conversationState.messages
    ) {
      conversationManager.addMessages(
        ...(workflowExecutionState.conversationState.messages as LLMMessage[]),
      );
    }

    // Step 10: Restore the index status
    if (workflowExecutionState.conversationState) {
      if (workflowExecutionState.conversationState.markMap) {
        conversationManager.setMarkMap(
          workflowExecutionState.conversationState.markMap as MessageMarkMap,
        );
      }

      // Recover Token Statistics
      conversationManager.setTokenUsageState(
        workflowExecutionState.conversationState.tokenUsage,
        workflowExecutionState.conversationState.currentRequestUsage,
      );
    }

    // Step 11: Create WorkflowStateCoordinator
    const stateCoordinator = new WorkflowStateCoordinator({
      workflowExecutionEntity,
      conversationManager,
    });

    // Step 12: Restore the trigger state
    if (workflowExecutionState.triggerStates) {
      const triggersArray = Array.from(workflowExecutionState.triggerStates.values());
      workflowExecutionEntity.restoreTriggerState({ triggers: triggersArray });
    }

    // Step 13: Reestablish the FORK/JOIN context (if it exists)
    if (workflowExecutionState.forkJoinContext) {
      workflowExecutionEntity.setForkId(workflowExecutionState.forkJoinContext.forkId);
      workflowExecutionEntity.setForkPathId(workflowExecutionState.forkJoinContext.forkPathId);
    }

    // Step 14: Restore the context of the Triggered sub-workflow (if any).
    if (workflowExecutionState.triggeredSubworkflowContext) {
      // Use new unified API to set parent context
      workflowExecutionEntity.setParentContext({
        parentType: 'WORKFLOW',
        parentId: workflowExecutionState.triggeredSubworkflowContext.parentExecutionId,
      });
      workflowExecutionEntity.setTriggeredSubworkflowId(
        workflowExecutionState.triggeredSubworkflowContext.triggeredSubworkflowId,
      );
    }

    // Step 15: Restore operation state for mid-node resume
    if (workflowExecutionState.currentOperation) {
      workflowExecutionEntity.state.restoreOperationState(workflowExecutionState.currentOperation);
      logger.info("Restored operation state from checkpoint", {
        executionId: workflowExecutionEntity.id,
        operationType: workflowExecutionState.currentOperation.type,
        operationId: workflowExecutionState.currentOperation.operationId,
      });
    }

    // Step 16: Infer FORK/JOIN completion status (if current node is JOIN)
    // Note: FORK/JOIN status does not need to be saved to Checkpoint and can be inferred from the execution registry during recovery
    if (workflowExecution.graph) {
      const currentNode = workflowExecution.graph.getNode(workflowExecutionState.currentNodeId);
      if (currentNode && currentNode.type === "JOIN") {
        const joinStatus = await this._inferForkJoinState(
          workflowExecutionState.currentNodeId,
          workflowExecutionEntity,
          dependencies.workflowExecutionRegistry,
        );
        
        logger.info("Inferred JOIN completion status", {
          executionId: workflowExecutionEntity.id,
          joinNodeId: workflowExecutionState.currentNodeId,
          completedPaths: Array.from(joinStatus.completedPaths),
          pendingPaths: Array.from(joinStatus.pendingPaths),
          failedPaths: Array.from(joinStatus.failedPaths),
        });
      }
    }

    // Step 17: Validate hierarchy integrity (if registry is available)
    const { hierarchyRegistry } = dependencies;
    if (hierarchyRegistry) {
      const hierarchyMetadata = workflowExecutionEntity.getHierarchyMetadata();
      if (hierarchyMetadata) {
        const validation = HierarchyIntegrityService.validateIntegrity(hierarchyMetadata, hierarchyRegistry);
        
        if (!validation.valid) {
          logger.warn('Hierarchy integrity issues detected after checkpoint restore', {
            executionId: workflowExecutionEntity.id,
            issues: validation.issues,
          });
          // Note: The entity's hierarchy manager will handle corrections when it accesses the registry
        }
      }
    }

    // Step 18: Reestablish the parent-child relationship for child workflows
    if (
      workflowExecutionState.triggeredSubworkflowContext?.childExecutionIds &&
      workflowExecutionState.triggeredSubworkflowContext.childExecutionIds.length > 0
    ) {
      for (const childWorkflowExecutionId of workflowExecutionState.triggeredSubworkflowContext
        .childExecutionIds) {
        // Find the Checkpoint of the SubWorkflowExecution
        const childCheckpointId = await this.findChildCheckpoint(
          childWorkflowExecutionId,
          checkpointStateManager,
        );
        if (childCheckpointId) {
          // Recover the sub-workflow execution
          const childResult = await this.restoreFromCheckpoint(childCheckpointId, dependencies);
          
          // Reestablish the parent-child relationship using new unified API
          childResult.workflowExecutionEntity.setParentContext({
            parentType: 'WORKFLOW',
            parentId: workflowExecutionEntity.id,
          });
          
          // Register with WorkflowExecutionRegistry
          workflowExecutionRegistry.register(childResult.workflowExecutionEntity);
          
          // Register the child workflow execution in the main workflow execution
          workflowExecutionEntity.registerChild({
            childType: 'WORKFLOW',
            childId: childWorkflowExecutionId,
            createdAt: Date.now(),
          });
        }
      }
    }

    // Step 19: Register with WorkflowExecutionRegistry
    workflowExecutionRegistry.register(workflowExecutionEntity);

    // Step 20: Restore file checkpoint (if file checkpoint manager is available)
    if (dependencies.fileCheckpointManager) {
      try {
        const fileCheckpoints = await dependencies.fileCheckpointManager
          .getStorage()
          .listByEntity(checkpoint.executionId, { limit: 1 });
        if (fileCheckpoints.length > 0) {
          const result = await dependencies.fileCheckpointManager.restoreCheckpoint(
            checkpoint.executionId,
            fileCheckpoints[0]!.id,
          );
          logger.info("File checkpoint restored alongside execution checkpoint", {
            executionId: checkpoint.executionId,
            restoredCount: result.restoredCount,
            deletedCount: result.deletedCount,
            skippedCount: result.skippedCount,
          });
        }
      } catch (error) {
        logger.error("File checkpoint restore failed (non-fatal)", { error });
      }
    }

    return {
      workflowExecutionEntity,
      stateCoordinator,
      conversationManager,
    };
  }

  /**
   * Create a node-level checkpoint (static method)
   * @param workflowExecutionId WorkflowExecution ID
   * @param nodeId Node ID
   * @param dependencies Dependencies
   * @param options Checkpoint options (metadata, description, tags, etc.)
   * @returns Checkpoint ID
   */
  static async createNodeCheckpoint(
    workflowExecutionId: string,
    nodeId: string,
    dependencies: CheckpointDependencies,
    options?: CheckpointOptions,
  ): Promise<string> {
    return CheckpointCoordinator.createCheckpoint(
      workflowExecutionId,
      dependencies,
      {
        ...options,
        nodeId,
        description: options?.description || `Node checkpoint for node ${nodeId}`,
      } as CheckpointOptions,
    );
  }

  /**
   * Create a tool-level checkpoint (static method)
   * @param workflowExecutionId WorkflowExecution ID
   * @param toolId Tool ID
   * @param dependencies Dependencies
   * @param options Checkpoint options (metadata, description, tags, etc.)
   * @returns Checkpoint ID
   */
  static async createToolCheckpoint(
    workflowExecutionId: string,
    toolId: string,
    dependencies: CheckpointDependencies,
    options?: CheckpointOptions,
  ): Promise<string> {
    return CheckpointCoordinator.createCheckpoint(
      workflowExecutionId,
      dependencies,
      {
        ...options,
        toolId,
        description: options?.description || `Tool checkpoint for tool ${toolId}`,
      } as CheckpointOptions,
    );
  }

  /**
   * Create checkpoints in batches (static method)
   * @param optionsList List of checkpoint creation options
   * @param dependencies Dependencies
   * @returns Array of checkpoint IDs
   */
  static async createCheckpoints(
    optionsList: CreateCheckpointOptions[],
    dependencies: CheckpointDependencies,
  ): Promise<string[]> {
    const promises = optionsList.map(options => CheckpointCoordinator.createCheckpoint(
      options.workflowExecutionId,
      dependencies,
      {
        metadata: options.metadata,
        description: options.description || `Checkpoint for execution ${options.workflowExecutionId}`,
        nodeId: options.nodeId,
        toolId: options.toolId,
        tags: options.tags,
        forceType: options.forceType,
        skipIf: options.skipIf,
      } as CheckpointOptions,
    ));
    return await Promise.all(promises);
  }

  /**
   * Infer FORK/JOIN completion status from JOIN node (static private method)
   * Determine the completion status of parallel branches by checking child workflow executions
   *
   * @param joinNodeId: ID of the JOIN node
   * @param workflowExecutionEntity: Current workflow execution entity
   * @param workflowExecutionRegistry: Registry to find child executions
   * @returns: Sets of completed, pending, and failed paths
   */
  private static async _inferForkJoinState(
    joinNodeId: string,
    workflowExecutionEntity: WorkflowExecutionEntity,
    workflowExecutionRegistry: WorkflowExecutionRegistry,
  ): Promise<{
    completedPaths: Set<string>;
    pendingPaths: Set<string>;
    failedPaths: Set<string>;
  }> {
    // Step 1: Get the JOIN node and its configuration
    const graph = workflowExecutionEntity.getGraph();
    const joinNode = graph.getNode(joinNodeId);
    
    if (!joinNode || joinNode.type !== "JOIN") {
      return {
        completedPaths: new Set(),
        pendingPaths: new Set(),
        failedPaths: new Set(),
      };
    }

    // Step 2: Extract forkPathIds from JOIN node configuration
    const joinConfig = joinNode.originalNode?.config as JoinNodeConfig | undefined;
    const forkPathIds = joinConfig?.forkPathIds || [];

    if (forkPathIds.length === 0) {
      logger.warn("JOIN node has no forkPathIds configured", {
        joinNodeId,
        executionId: workflowExecutionEntity.id,
      });
      return {
        completedPaths: new Set(),
        pendingPaths: new Set(),
        failedPaths: new Set(),
      };
    }

    // Step 3: Find all FORK_JOIN child executions from registry
    const allExecutions = workflowExecutionRegistry.getAll();
    const parentExecutionId = workflowExecutionEntity.id;
    
    // Filter child executions that belong to this parent and are FORK_JOIN type
    const childExecutions = Array.from(allExecutions.values()).filter((exec) => {
      const execData = exec.getWorkflowExecutionData();
      return (
        execData.executionType === "FORK_JOIN" &&
        execData.hierarchy?.parent?.parentId === parentExecutionId
      );
    });

    // Step 4: Determine completion status for each path
    const completedPaths = new Set<string>();
    const pendingPaths = new Set<string>();
    const failedPaths = new Set<string>();

    for (const pathId of forkPathIds) {
      // Find the child execution corresponding to this pathId
      const childExecution = childExecutions.find(
        (exec) => exec.getWorkflowExecutionData().forkJoinContext?.forkPathId === pathId,
      );

      if (!childExecution) {
        // No child execution found for this path - still pending
        pendingPaths.add(pathId);
      } else {
        // Check the status of the child execution
        const status = childExecution.getStatus();
        
        if (status === "COMPLETED") {
          completedPaths.add(pathId);
        } else if (status === "FAILED" || status === "CANCELLED" || status === "TIMEOUT") {
          failedPaths.add(pathId);
        } else {
          // RUNNING, PAUSED, STOPPED, CREATED are considered pending
          pendingPaths.add(pathId);
        }
      }
    }

    return { completedPaths, pendingPaths, failedPaths };
  }

  /**
   * Find the Checkpoint ID of a SubWorkflowExecution (static private method)
   * @param childWorkflowExecutionId: SubWorkflowExecution ID
   * @param checkpointStateManager: Checkpoint state manager
   * @returns: Checkpoint ID; returns undefined if not found
   */
  private static async findChildCheckpoint(
    childWorkflowExecutionId: string,
    checkpointStateManager: CheckpointState,
  ): Promise<string | undefined> {
    // Get all Checkpoints for this WorkflowExecution
    const checkpointIds = await checkpointStateManager.list({ parentId: childWorkflowExecutionId });
    if (checkpointIds.length === 0) {
      return undefined;
    }
    // Return the latest checkpoint (the first one).
    return checkpointIds[0];
  }

  /**
   * Verify the integrity and compatibility of checkpoints (static private methods)
   */
  private static validateCheckpoint(checkpoint: Checkpoint): void {
    // Store common fields before type narrowing
    const checkpointId = checkpoint.id;
    const workflowId = checkpoint.workflowId;
    const executionId = checkpoint.executionId;

    // Verify required fields
    if (!checkpointId || !executionId || !workflowId) {
      throw new WorkflowCheckpointError(
        "Invalid checkpoint: missing required fields",
        "validate",
        checkpointId,
        undefined,
        workflowId,
        executionId,
      );
    }

    // Store the type for error reporting
    const checkpointType = checkpoint.type;

    // Verify according to the checkpoint type.
    if (checkpointType === "FULL") {
      const fullCp = checkpoint as FullCheckpoint<WorkflowExecutionStateSnapshot>;
      if (!fullCp.snapshot) {
        throw new WorkflowCheckpointError(
          "Invalid full checkpoint: missing snapshot",
          "validate",
          checkpointId,
          undefined,
          workflowId,
          executionId,
        );
      }
    } else if (checkpointType === "DELTA") {
      const deltaCp = checkpoint as DeltaCheckpoint<CheckpointDelta>;
      if (!deltaCp.delta) {
        throw new WorkflowCheckpointError(
          "Invalid delta checkpoint: missing delta",
          "validate",
          checkpointId,
          undefined,
          workflowId,
          executionId,
        );
      }
      if (!deltaCp.baseCheckpointId) {
        throw new WorkflowCheckpointError(
          "Invalid delta checkpoint: missing baseCheckpointId",
          "validate",
          checkpointId,
          undefined,
          workflowId,
          executionId,
        );
      }
    } else {
      throw new WorkflowCheckpointError(
        `Invalid checkpoint type: ${checkpointType}`,
        "validate",
        checkpointId,
        undefined,
        workflowId,
        executionId,
      );
    }
  }
}
