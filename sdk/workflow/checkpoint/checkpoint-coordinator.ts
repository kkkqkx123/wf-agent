/**
 * Checkpoint Coordinator
 * A stateless service that coordinates the entire checkpoint process
 */

import {
  WorkflowExecutionNotFoundError,
  CheckpointNotFoundError,
  WorkflowNotFoundError,
} from "@wf-agent/types";
import { CheckpointType } from "@wf-agent/types";
import { DEFAULT_DELTA_STORAGE_CONFIG } from "../../core/utils/checkpoint/constants.js";
import type { WorkflowExecution } from "@wf-agent/types";
import type {
  Checkpoint,
  CheckpointMetadata,
  WorkflowExecutionStateSnapshot,
  MessageMarkMap,
  DeltaStorageConfig,
  TCheckpointType,
  WorkflowExecutionVariable,
  LLMMessage,
  NodeExecutionResult,
  TriggerRuntimeState,
} from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../stores/workflow-execution-registry.js";
import type { WorkflowRegistry } from "../stores/workflow-registry.js";
import type { WorkflowGraphRegistry } from "../stores/workflow-graph-registry.js";
import { CheckpointState } from "./checkpoint-state-manager.js";
import { ConversationSession } from "../../core/messaging/conversation-session.js";
import { VariableState } from "../state-managers/variable-state.js";
import { WorkflowExecutionEntity } from "../entities/workflow-execution-entity.js";
import { ExecutionState } from "../state-managers/execution-state.js";
import { WorkflowStateCoordinator } from "../state-managers/workflow-state-coordinator.js";
import { CheckpointDiffCalculator } from "./utils/diff-calculator.js";
import { DeltaCheckpointRestorer } from "./utils/delta-restorer.js";
import { generateId } from "../../utils/index.js";
import { now } from "@wf-agent/common-utils";
import { mergeMetadata } from "../../utils/metadata-utils.js";
import type { Metadata } from "@wf-agent/types";
import type { ExecutionHierarchyRegistry } from "../../core/registry/execution-hierarchy-registry.js";
import { HierarchyIntegrityService } from "../../core/execution/hierarchy-integrity-service.js";

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
}

/**
 * Checkpoint Coordinator (completely stateless)
 */
export class CheckpointCoordinator {
  private static diffCalculator = new CheckpointDiffCalculator();

  /**
   * Create a checkpoint (static method)
   * @param workflowExecutionId WorkflowExecution ID
   * @param dependencies Dependencies
   * @param metadata Checkpoint metadata
   * @param conversationManager Optional ConversationSession (if not using stateCoordinatorMap)
   * @returns Checkpoint ID
   */
  static async createCheckpoint(
    workflowExecutionId: string,
    dependencies: CheckpointDependencies,
    metadata?: CheckpointMetadata,
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
    const checkpointType = CheckpointCoordinator.determineCheckpointType(checkpointCount, config);

    // Step 5: Generate a unique checkpointId and timestamp
    const checkpointId = generateId();
    const timestamp = now();

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
          const restorer = new DeltaCheckpointRestorer(
            id => checkpointStateManager.get(id),
            workflowExecutionId => checkpointStateManager.list({ parentId: workflowExecutionId }),
          );
          const restoreResult = await restorer.restore(previousCheckpointId);
          previousState = restoreResult.snapshot;
        } else {
          const fullCp = previousCheckpoint as import("@wf-agent/types").FullCheckpoint<WorkflowExecutionStateSnapshot>;
          previousState = fullCp.snapshot;
        }

        // Calculate the difference
        const delta = CheckpointCoordinator.diffCalculator.calculateDelta(
          previousState,
          currentState,
        );

        // Find the baseline checkpoint ID
        let baseCheckpointId: string;
        if (previousCheckpoint.type === "FULL") {
          baseCheckpointId = previousCheckpoint.id;
        } else {
          const deltaCp = previousCheckpoint as import("@wf-agent/types").DeltaCheckpoint<import("@wf-agent/types").CheckpointDelta>;
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
    return await checkpointStateManager.create(checkpoint);
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
    // Create a variable snapshot using VariableState
    const variableStateManager = new VariableState();
    const variableSnapshot = variableStateManager.createSnapshot();

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

    return {
      status: workflowExecutionEntity.getStatus(),
      currentNodeId: workflowExecution.currentNodeId,
      variables: variableSnapshot.variables,
      variableScopes: variableSnapshot.variableScopes,
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
      const restorer = new DeltaCheckpointRestorer(
        id => checkpointStateManager.get(id),
        workflowExecutionId => checkpointStateManager.list({ parentId: workflowExecutionId }),
      );
      const restoreResult = await restorer.restore(checkpointId);
      workflowExecutionState = restoreResult.snapshot;
    } else {
      // Full checkpoint, use it directly.
      const fullCp = checkpoint as import("@wf-agent/types").FullCheckpoint<WorkflowExecutionStateSnapshot>;
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
      variableScopes: workflowExecutionState.variableScopes,
      graph,
    };

    // Step 6: Restore the variable snapshot using VariableState
    const variableStateManager = new VariableState();
    variableStateManager.restoreFromSnapshot({
      variables: workflowExecutionState.variables as WorkflowExecutionVariable[],
      variableScopes: workflowExecutionState.variableScopes,
    });

    // Step 7: Create WorkflowExecutionEntity (without ConversationManager)
    const executionState = new ExecutionState();
    const workflowExecutionEntity = new WorkflowExecutionEntity(
      workflowExecution as WorkflowExecution,
      executionState,
    );

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
      workflowExecutionEntity.setParentExecutionId(
        workflowExecutionState.triggeredSubworkflowContext.parentExecutionId,
      );
      workflowExecutionEntity.setTriggeredSubworkflowId(
        workflowExecutionState.triggeredSubworkflowContext.triggeredSubworkflowId,
      );
    }

    // Step 15: Inferring FORK/JOIN status (if needed)
    // Note: FORK/JOIN status does not need to be saved to Checkpoint and can be inferred from the diagram structure and execution sequence during recovery
    // If the current node is a JOIN node, you can infer which branches have completed
    if (workflowExecution.graph) {
      const currentNode = workflowExecution.graph.getNode(workflowExecutionState.currentNodeId);
      if (currentNode && currentNode.type === "JOIN") {
        // Here, corresponding processing can be carried out based on the inferred state
        // For example: logging or updating certain status
      }
    }

    // Step 16: Validate hierarchy integrity (if registry is available)
    const { hierarchyRegistry } = dependencies;
    if (hierarchyRegistry) {
      const hierarchyMetadata = workflowExecutionEntity.getHierarchyMetadata();
      if (hierarchyMetadata) {
        const validation = HierarchyIntegrityService.validateIntegrity(hierarchyMetadata, hierarchyRegistry);
        
        if (!validation.valid) {
          console.warn('Hierarchy integrity issues detected after checkpoint restore', {
            executionId: workflowExecutionEntity.id,
            issues: validation.issues,
          });
          // Note: The entity's hierarchy manager will handle corrections when it accesses the registry
        }
      }
    }

    // Step 17: Reestablish the parent-child relationship for child workflows
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
          // Reestablish the parent-child relationship
          childResult.workflowExecutionEntity.setParentExecutionId(workflowExecutionEntity.id);
          // Register with WorkflowExecutionRegistry
          workflowExecutionRegistry.register(childResult.workflowExecutionEntity);
          // Register the child workflow execution in the main workflow execution.
          workflowExecutionEntity.registerChildExecution(childWorkflowExecutionId);
        }
      }
    }

    // Step 17: Register with WorkflowExecutionRegistry
    workflowExecutionRegistry.register(workflowExecutionEntity);

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
   * @param metadata Checkpoint metadata
   * @param dependencies Dependencies
   * @returns Checkpoint ID
   */
  static async createNodeCheckpoint(
    workflowExecutionId: string,
    nodeId: string,
    dependencies: CheckpointDependencies,
    metadata?: CheckpointMetadata,
  ): Promise<string> {
    return CheckpointCoordinator.createCheckpoint(
      workflowExecutionId,
      dependencies,
      mergeMetadata((metadata as Metadata) || {}, {
        description: metadata?.description || `Node checkpoint for node ${nodeId}`,
        customFields: mergeMetadata((metadata?.customFields as Metadata) || {}, { nodeId }),
      }),
    );
  }

  /**
   * Infer FORK/JOIN status (static private method)
   * Determine the completion status of parallel branches from the graph structure and execution sequence
   *
   * @param forkNodeId: ID of the FORK node
   * @param nodeResults: Execution results of the node
   * @param graph: Workflow graph
   * @returns: Set of completed and incomplete paths
   */
  private static inferForkJoinState(
    forkNodeId: string,
    nodeResults: Record<string, unknown>,
    graph: unknown,
  ): {
    completedPaths: Set<string>;
    pendingPaths: Set<string>;
  } {
    // 1. Obtain the FORK node
    const graphObj = graph as {
      getNode: (id: string) => { type: string; config?: unknown } | null;
    };
    const forkNode = graphObj.getNode(forkNodeId);
    if (!forkNode || forkNode.type !== "FORK") {
      return { completedPaths: new Set(), pendingPaths: new Set() };
    }

    // 2. Obtain all paths for the FORK node
    const forkPaths =
      (forkNode.config as { forkPaths?: { pathId: string; childNodeId: string }[] })?.forkPaths ||
      [];

    // 3. Determine which paths have been completed
    const completedPaths = new Set<string>();
    const pendingPaths = new Set<string>();

    for (const forkPath of forkPaths) {
      const pathId = forkPath.pathId;
      const startNodeId = forkPath.childNodeId;

      if (nodeResults[startNodeId]) {
        completedPaths.add(pathId);
      } else {
        pendingPaths.add(pathId);
      }
    }

    return { completedPaths, pendingPaths };
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
    // Verify required fields
    if (!checkpoint.id || !checkpoint.executionId || !checkpoint.workflowId) {
      throw new Error("Invalid checkpoint: missing required fields");
    }

    // Store the type for error reporting
    const checkpointType = checkpoint.type;

    // Verify according to the checkpoint type.
    if (checkpointType === "FULL") {
      const fullCp = checkpoint as import("@wf-agent/types").FullCheckpoint<WorkflowExecutionStateSnapshot>;
      if (!fullCp.snapshot) {
        throw new Error("Invalid full checkpoint: missing snapshot");
      }
    } else if (checkpointType === "DELTA") {
      const deltaCp = checkpoint as import("@wf-agent/types").DeltaCheckpoint<import("@wf-agent/types").CheckpointDelta>;
      if (!deltaCp.delta) {
        throw new Error("Invalid delta checkpoint: missing delta");
      }
      if (!deltaCp.baseCheckpointId) {
        throw new Error("Invalid delta checkpoint: missing baseCheckpointId");
      }
    } else {
      throw new Error(`Invalid checkpoint type: ${checkpointType}`);
    }
  }
}
