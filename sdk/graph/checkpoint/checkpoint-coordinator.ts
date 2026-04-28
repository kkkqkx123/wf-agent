/**
 * Checkpoint Coordinator
 * A stateless service that coordinates the entire checkpoint process
 */

import {
  ThreadContextNotFoundError,
  CheckpointNotFoundError,
  WorkflowNotFoundError,
} from "@wf-agent/types";
import { CheckpointType } from "@wf-agent/types";
import { DEFAULT_DELTA_STORAGE_CONFIG } from "../../core/utils/checkpoint/constants.js";
import type { Thread } from "@wf-agent/types";
import type {
  Checkpoint,
  CheckpointMetadata,
  ThreadStateSnapshot,
  MessageMarkMap,
  DeltaStorageConfig,
  TCheckpointType,
  ThreadVariable,
  LLMMessage,
  NodeExecutionResult,
  TriggerRuntimeState,
} from "@wf-agent/types";
import type { ThreadRegistry } from "../stores/thread-registry.js";
import type { WorkflowRegistry } from "../stores/workflow-registry.js";
import type { GraphRegistry } from "../stores/graph-registry.js";
import { CheckpointState } from "./checkpoint-state-manager.js";
import { ConversationSession } from "../../core/messaging/conversation-session.js";
import { VariableState } from "../state-managers/variable-state.js";
import { ThreadEntity } from "../entities/thread-entity.js";
import { ExecutionState } from "../state-managers/execution-state.js";
import { ThreadStateCoordinator } from "../state-managers/thread-state-coordinator.js";
import { CheckpointDiffCalculator } from "./utils/diff-calculator.js";
import { DeltaCheckpointRestorer } from "./utils/delta-restorer.js";
import { generateId } from "../../utils/index.js";
import { now } from "@wf-agent/common-utils";
import { mergeMetadata } from "../../utils/metadata-utils.js";
import type { Metadata } from "@wf-agent/types";

/**
 * Checkpoint dependencies
 */
export interface CheckpointDependencies {
  threadRegistry: ThreadRegistry;
  checkpointStateManager: CheckpointState;
  workflowRegistry: WorkflowRegistry;
  graphRegistry: GraphRegistry;
  /** Incremental storage configuration (optional) */
  deltaConfig?: DeltaStorageConfig;
  /** ThreadStateCoordinator map (optional, for new architecture) */
  stateCoordinatorMap?: Map<string, ThreadStateCoordinator>;
}

/**
 * Checkpoint Coordinator (completely stateless)
 */
export class CheckpointCoordinator {
  private static diffCalculator = new CheckpointDiffCalculator();

  /**
   * Create a checkpoint (static method)
   * @param threadId Thread ID
   * @param dependencies Dependencies
   * @param metadata Checkpoint metadata
   * @param conversationManager Optional ConversationSession (if not using stateCoordinatorMap)
   * @returns Checkpoint ID
   */
  static async createCheckpoint(
    threadId: string,
    dependencies: CheckpointDependencies,
    metadata?: CheckpointMetadata,
    conversationManager?: ConversationSession,
  ): Promise<string> {
    const { threadRegistry, checkpointStateManager, deltaConfig, stateCoordinatorMap } =
      dependencies;
    const config = { ...DEFAULT_DELTA_STORAGE_CONFIG, ...deltaConfig };

    // Step 1: Retrieve the ThreadEntity object from the ThreadRegistry
    const threadEntity = threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new ThreadContextNotFoundError(`ThreadEntity not found`, threadId);
    }

    const thread = threadEntity.getThread();

    // Get ConversationSession from stateCoordinatorMap or parameter
    const convManager =
      conversationManager ?? stateCoordinatorMap?.get(threadId)?.getConversationManager();

    // Step 2: Extract ThreadStateSnapshot
    const currentState = CheckpointCoordinator.extractThreadState(
      threadEntity,
      thread,
      convManager,
    );

    // Step 3: Retrieve the previous checkpoint
    const previousCheckpointIds = await checkpointStateManager.list({ parentId: threadId });
    const checkpointCount = previousCheckpointIds.length;

    // Step 4: Determine the type of checkpoint
    const checkpointType = CheckpointCoordinator.determineCheckpointType(checkpointCount, config);

    // Step 5: Generate a unique checkpointId and timestamp
    const checkpointId = generateId();
    const timestamp = now();

    // Step 6: Create a checkpoint
    let checkpoint: Checkpoint;

    if (checkpointType === CheckpointType["FULL"]) {
      // Create a complete checkpoint
      checkpoint = {
        id: checkpointId,
        threadId: threadEntity.id,
        workflowId: threadEntity.getWorkflowId(),
        timestamp,
        type: CheckpointType["FULL"]!,
        threadState: currentState,
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
          threadId: threadEntity.id,
          workflowId: threadEntity.getWorkflowId(),
          timestamp,
          type: CheckpointType["FULL"]!,
          threadState: currentState,
          metadata,
        };
      } else {
        // Get the complete status of the previous checkpoint.
        let previousState: ThreadStateSnapshot;
        if (previousCheckpoint.type === CheckpointType["DELTA"]) {
          // If the previous checkpoint was an incremental checkpoint, the full state needs to be restored.
          const restorer = new DeltaCheckpointRestorer(
            id => checkpointStateManager.get(id),
            threadId => checkpointStateManager.list({ parentId: threadId }),
          );
          const restoreResult = await restorer.restore(previousCheckpointId);
          previousState = restoreResult.snapshot;
        } else {
          previousState = previousCheckpoint.threadState!;
        }

        // Calculate the difference
        const delta = CheckpointCoordinator.diffCalculator.calculateDelta(
          previousState,
          currentState,
        );

        // Find the baseline checkpoint ID
        let baseCheckpointId = previousCheckpoint.baseCheckpointId;
        if (!baseCheckpointId && previousCheckpoint.type === CheckpointType["FULL"]) {
          baseCheckpointId = previousCheckpoint.id;
        }

        checkpoint = {
          id: checkpointId,
          threadId: threadEntity.id,
          workflowId: threadEntity.getWorkflowId(),
          timestamp,
          type: CheckpointType["DELTA"]!,
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
   * Extract a thread state snapshot
   * @param threadEntity Thread entity
   * @param thread Thread object
   * @param conversationManager Optional ConversationSession
   * @returns Thread state snapshot
   */
  private static extractThreadState(
    threadEntity: ThreadEntity,
    thread: Thread,
    conversationManager?: ConversationSession,
  ): ThreadStateSnapshot {
    // Create a variable snapshot using VariableState
    const variableStateManager = new VariableState();
    const variableSnapshot = variableStateManager.createSnapshot();

    // Convert the `nodeResults` array into Record format.
    const nodeResultsRecord: Record<string, NodeExecutionResult> = {};
    for (const result of thread.nodeResults) {
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
    const triggerStateSnapshot = threadEntity.getTriggerStateSnapshot();

    return {
      status: threadEntity.getStatus(),
      currentNodeId: thread.currentNodeId,
      variables: variableSnapshot.variables,
      variableScopes: variableSnapshot.variableScopes,
      input: thread.input,
      output: thread.output,
      nodeResults: nodeResultsRecord,
      errors: thread.errors,
      conversationState,
      triggerStates:
        triggerStateSnapshot.triggers.length > 0
          ? (new Map(Object.entries(triggerStateSnapshot.triggers)) as Map<
              string,
              TriggerRuntimeState
            >)
          : undefined,
      forkJoinContext: thread.forkJoinContext,
      triggeredSubworkflowContext: thread.triggeredSubworkflowContext,
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
      return CheckpointType["FULL"]!;
    }

    // The first checkpoint must be a complete checkpoint.
    if (checkpointCount === 0) {
      return CheckpointType["FULL"]!;
    }

    // Create a complete checkpoint every baselineInterval checkpoints.
    if (checkpointCount % config.baselineInterval === 0) {
      return CheckpointType["FULL"]!;
    }

    // Create incremental checkpoints in other cases.
    return CheckpointType["DELTA"]!;
  }

  /**
   * Restore the ThreadEntity state from a checkpoint (static method)
   * @param checkpointId Checkpoint ID
   * @param dependencies Dependencies
   * @returns ThreadBuildResult containing ThreadEntity, ThreadStateCoordinator, and ConversationSession
   */
  static async restoreFromCheckpoint(
    checkpointId: string,
    dependencies: CheckpointDependencies,
  ): Promise<{
    threadEntity: ThreadEntity;
    stateCoordinator: ThreadStateCoordinator;
    conversationManager: ConversationSession;
  }> {
    const { threadRegistry, checkpointStateManager, graphRegistry } = dependencies;

    // Step 1: Load the checkpoint from CheckpointState
    const checkpoint = await checkpointStateManager.get(checkpointId);
    if (!checkpoint) {
      throw new CheckpointNotFoundError(`Checkpoint not found`, checkpointId);
    }

    // Step 2: Verify the integrity and compatibility of the checkpoint
    CheckpointCoordinator.validateCheckpoint(checkpoint);

    // Step 3: Obtain the complete thread state (processing incremental checkpoints)
    let threadState: ThreadStateSnapshot;
    if (checkpoint.type === CheckpointType["DELTA"]) {
      // If it's an incremental checkpoint, the full state needs to be restored.
      const restorer = new DeltaCheckpointRestorer(
        id => checkpointStateManager.get(id),
        threadId => checkpointStateManager.list({ parentId: threadId }),
      );
      const restoreResult = await restorer.restore(checkpointId);
      threadState = restoreResult.snapshot;
    } else {
      // Full checkpoint, use it directly.
      threadState = checkpoint.threadState!;
    }

    // Step 4: Get the PreprocessedGraph from the GraphRegistry
    // The PreprocessedGraph contains the complete preprocessed graph structure
    const processedWorkflow = graphRegistry.get(checkpoint.workflowId);
    if (!processedWorkflow) {
      throw new WorkflowNotFoundError(`Processed workflow not found`, checkpoint.workflowId);
    }

    // The PreprocessedGraph itself is a Graph, containing the complete graph structure.
    // Design purpose: The restored Thread requires a complete graph structure
    // Continue to execute the workflow (for example: finding nodes, traversing edges, performing graph algorithms, etc.)
    const graph = processedWorkflow;

    // Step 5: Restore Thread state
    // Convert nodeResults Record back to array format
    const nodeResultsArray = Object.values(threadState.nodeResults || {});

    const thread: Partial<Thread> = {
      id: checkpoint.threadId,
      workflowId: checkpoint.workflowId,
      workflowVersion: "1.0.0", // TODO: Retrieve the version from the checkpoint metadata
      currentNodeId: threadState.currentNodeId,
      input: threadState.input,
      output: threadState.output,
      nodeResults: nodeResultsArray,
      errors: threadState.errors,
      forkJoinContext: threadState.forkJoinContext,
      triggeredSubworkflowContext: threadState.triggeredSubworkflowContext,
      variableScopes: threadState.variableScopes,
      graph,
    };

    // Step 6: Restore the variable snapshot using VariableState
    const variableStateManager = new VariableState();
    variableStateManager.restoreFromSnapshot({
      variables: threadState.variables as ThreadVariable[],
      variableScopes: threadState.variableScopes,
    });

    // Step 7: Create ThreadEntity (without ConversationManager)
    const executionState = new ExecutionState();
    const threadEntity = new ThreadEntity(thread as Thread, executionState);

    // Step 8: Create the ConversationSession
    const conversationManager = new ConversationSession();

    // Step 9: Restore the complete message history from the checkpoint snapshot
    if (threadState.conversationState && threadState.conversationState.messages) {
      conversationManager.addMessages(...(threadState.conversationState.messages as LLMMessage[]));
    }

    // Step 10: Restore the index status
    if (threadState.conversationState) {
      if (threadState.conversationState.markMap) {
        conversationManager.setMarkMap(threadState.conversationState.markMap as MessageMarkMap);
      }

      // Recover Token Statistics
      conversationManager.setTokenUsageState(
        threadState.conversationState.tokenUsage,
        threadState.conversationState.currentRequestUsage,
      );
    }

    // Step 11: Create ThreadStateCoordinator
    const stateCoordinator = new ThreadStateCoordinator({
      threadEntity,
      conversationManager,
    });

    // Step 12: Restore the trigger state
    if (threadState.triggerStates) {
      const triggersArray = Array.from(threadState.triggerStates.values());
      threadEntity.restoreTriggerState({ triggers: triggersArray });
    }

    // Step 13: Reestablish the FORK/JOIN context (if it exists)
    if (threadState.forkJoinContext) {
      threadEntity.setForkId(threadState.forkJoinContext.forkId);
      threadEntity.setForkPathId(threadState.forkJoinContext.forkPathId);
    }

    // Step 14: Restore the context of the Triggered sub-workflow (if any).
    if (threadState.triggeredSubworkflowContext) {
      threadEntity.setParentThreadId(threadState.triggeredSubworkflowContext.parentThreadId);
      threadEntity.setTriggeredSubworkflowId(
        threadState.triggeredSubworkflowContext.triggeredSubworkflowId,
      );
    }

    // Step 15: Inferring FORK/JOIN status (if needed)
    // Note: FORK/JOIN status does not need to be saved to Checkpoint and can be inferred from the diagram structure and execution sequence during recovery
    // If the current node is a JOIN node, you can infer which branches have completed
    if (thread.graph) {
      const currentNode = thread.graph.getNode(threadState.currentNodeId);
      if (currentNode && currentNode.type === "JOIN") {
        // Here, corresponding processing can be carried out based on the inferred state
        // For example: logging or updating certain status
      }
    }

    // Step 16: Reverting to the sub-thread (Option 3: Master-Slave Separation Mode)
    if (
      threadState.triggeredSubworkflowContext?.childThreadIds &&
      threadState.triggeredSubworkflowContext.childThreadIds.length > 0
    ) {
      for (const childThreadId of threadState.triggeredSubworkflowContext.childThreadIds) {
        // Find the Checkpoint of the SubThread
        const childCheckpointId = await this.findChildCheckpoint(
          childThreadId,
          checkpointStateManager,
        );
        if (childCheckpointId) {
          // Recover the sub-thread
          const childResult = await this.restoreFromCheckpoint(childCheckpointId, dependencies);
          // Reestablish the parent-child relationship
          childResult.threadEntity.setParentThreadId(threadEntity.id);
          // Register with ThreadRegistry
          threadRegistry.register(childResult.threadEntity);
          // Register the child thread in the main thread.
          threadEntity.registerChildThread(childThreadId);
        }
      }
    }

    // Step 17: Register with ThreadRegistry
    threadRegistry.register(threadEntity);

    return {
      threadEntity,
      stateCoordinator,
      conversationManager,
    };
  }

  /**
   * Create a node-level checkpoint (static method)
   * @param threadId Thread ID
   * @param nodeId Node ID
   * @param metadata Checkpoint metadata
   * @param dependencies Dependencies
   * @returns Checkpoint ID
   */
  static async createNodeCheckpoint(
    threadId: string,
    nodeId: string,
    dependencies: CheckpointDependencies,
    metadata?: CheckpointMetadata,
  ): Promise<string> {
    return CheckpointCoordinator.createCheckpoint(
      threadId,
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
   * Find the Checkpoint ID of a SubThread (static private method)
   * @param childThreadId: SubThread ID
   * @param checkpointStateManager: Checkpoint state manager
   * @returns: Checkpoint ID; returns undefined if not found
   */
  private static async findChildCheckpoint(
    childThreadId: string,
    checkpointStateManager: CheckpointState,
  ): Promise<string | undefined> {
    // Get all Checkpoints for this Thread
    const checkpointIds = await checkpointStateManager.list({ parentId: childThreadId });
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
    if (!checkpoint.id || !checkpoint.threadId || !checkpoint.workflowId) {
      throw new Error("Invalid checkpoint: missing required fields");
    }

    // Verify according to the checkpoint type.
    if (checkpoint.type === CheckpointType["DELTA"]) {
      // The incremental checkpoint needs to verify the delta field.
      if (!checkpoint.delta && !checkpoint.previousCheckpointId) {
        throw new Error(
          "Invalid delta checkpoint: missing delta data and previous checkpoint reference",
        );
      }
    } else {
      // The complete checkpoint requires verification of the threadState field.
      if (!checkpoint.threadState) {
        throw new Error("Invalid full checkpoint: missing thread state");
      }

      // Verify the threadState structure.
      const { threadState } = checkpoint;
      if (!threadState.status || !threadState.currentNodeId) {
        throw new Error("Invalid checkpoint: incomplete thread state");
      }
    }
  }
}
