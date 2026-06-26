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
  CheckpointErrorStrategy,
  CheckpointErrorContext,
  CheckpointFormatVersion,
} from "@wf-agent/types";
import type { WorkflowExecutionRegistry } from "../stores/workflow-execution-registry.js";
import type { WorkflowRegistry } from "../stores/workflow-registry.js";
import type { WorkflowGraphRegistry } from "../stores/workflow-graph-registry.js";
import type { JoinNodeConfig } from "@wf-agent/types";
import { CheckpointState } from "./checkpoint-state-manager.js";
import { ConversationSession } from "../../shared/messaging/conversation-session.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { WorkflowExecutionEntity } from "../entities/workflow-execution-entity.js";
import { ExecutionState } from "../state-managers/execution-state.js";
import { WorkflowStateCoordinator } from "../state-managers/workflow-state-coordinator.js";
import { BaseDeltaRestorer } from "../../shared/checkpoint/base-delta-restorer.js";
import { buildCheckpointMetadata } from "../../shared/checkpoint/utils/metadata-builder.js";
import { BaseCheckpointCoordinator } from "../../shared/checkpoint/base-checkpoint-coordinator.js";
import type { CheckpointDependencies as BaseCheckpointDependencies } from "../../shared/checkpoint/types.js";
import type { ExecutionHierarchyRegistry } from "../../shared/registry/execution-hierarchy-registry.js";
import type { AnyExecutionEntity } from "../../shared/registry/execution-hierarchy-registry.js";
import type { FileCheckpointManager } from "@wf-agent/common-utils";
import { HierarchyIntegrityService } from "../../shared/execution/hierarchy-integrity-service.js";
import { CheckpointErrorHandler } from "../../shared/checkpoint/checkpoint-error-handler.js";
import { CheckpointVersionManager } from "../../shared/checkpoint/checkpoint-version-manager.js";
import { CURRENT_CHECKPOINT_FORMAT_VERSION } from "@wf-agent/types";
import { getExecutionEventBus } from "../../shared/events/index.js";
import { ChildCheckpointRestorer } from "../../shared/checkpoint/child-checkpoint-restorer.js";
import type { ChildRestoreDependencies } from "../../shared/checkpoint/child-checkpoint-restorer.js";
import type { AgentLoopCheckpointCoordinator, CheckpointDependencies as AgentCheckpointDependencies } from "../../agent/checkpoint/checkpoint-coordinator.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import type { ChildCheckpointResolver } from "../../shared/checkpoint/child-checkpoint-resolver.js";

const logger = createContextualLogger({ component: "CheckpointCoordinator" });

/**
 * Agent-specific dependencies for cross-type child restoration.
 * Used by Workflow coordinator to restore AGENT_LOOP children.
 */
export interface AgentChildRestoreDeps {
  agentCoordinator: AgentLoopCheckpointCoordinator;
  agentDeps: AgentCheckpointDependencies;
  agentRuntimeConfig: AgentLoopRuntimeConfig;
}

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
  /** Conversation manager for message persistence (optional - can be provided here or as parameter) */
  conversationManager?: ConversationSession;
  /** Agent-specific dependencies for restoring AGENT_LOOP children */
  agentChildDeps?: AgentChildRestoreDeps;
  /** Custom checkpoint resolver for finding latest child checkpoints */
  childCheckpointResolver?: ChildCheckpointResolver;
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
interface WorkflowRestoreContext {
  conversationManager: ConversationSession;
  stateCoordinator: WorkflowStateCoordinator;
  checkpoint: Checkpoint;
  restoredSnapshot: WorkflowExecutionStateSnapshot;
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
  private checkpointErrorHandler?: CheckpointErrorHandler;
  private checkpointErrorStrategy: CheckpointErrorStrategy = "warn";
  /**
   * Version manager for format compatibility and migration
   */
  private versionManager: CheckpointVersionManager;

  constructor() {
    super();
    this.versionManager = new CheckpointVersionManager(logger);
  }

  // ============================================================================
  // Public Instance Methods - Workflow-specific entry points
  // ============================================================================

  /**
   * Create a workflow checkpoint
   * @param entity Workflow execution entity
   * @param dependencies Workflow checkpoint dependencies
   * @param options Checkpoint options
   * @param conversationManager Optional conversation manager (parameter takes precedence over dependencies)
   * @returns Checkpoint ID
   */
  async createWorkflowCheckpoint(
    entity: WorkflowExecutionEntity,
    dependencies: WorkflowCheckpointDependencies,
    options?: CheckpointOptions,
    conversationManager?: ConversationSession,
  ): Promise<string> {
    this.ensureDeps(dependencies);

    const resolvedConversationManager =
      conversationManager ??
      dependencies.conversationManager ??
      dependencies.stateCoordinatorMap?.get(entity.id)?.getConversationManager();

    if (!resolvedConversationManager) {
      logger.warn("No conversationManager available, checkpoint will have empty messages", {
        executionId: entity.id,
      });
    }

    const metadata = this.buildMetadata(options);

    const checkpointId = await super.createCheckpoint(
      entity,
      this.toBaseDeps(dependencies),
      metadata,
      { conversationManager: resolvedConversationManager },
    );

    // Publish checkpoint state change event (Plan C)
    const eventBus = getExecutionEventBus();
    await eventBus.publish({
      type: "state_changed",
      executionId: entity.id,
      timestamp: Date.now(),
      newStatus: entity.getStatus(),
      changes: {
        checkpointCreated: checkpointId,
        description: options?.description,
        nodeId: options?.nodeId,
        toolId: options?.toolId,
        tags: options?.tags,
      },
    });

    // Create file checkpoint if manager is available
    if (dependencies.fileCheckpointManager) {
      try {
        await dependencies.fileCheckpointManager.createCheckpoint(entity.id);
        logger.info("File checkpoint created alongside execution checkpoint", {
          executionId: entity.id,
        });
      } catch (error) {
        await this.handleFileCheckpointError(
          error as Error,
          "create",
          entity.id,
          checkpointId,
        );
      }
    }

    return checkpointId;
  }

  /**
   * Restore workflow from checkpoint (workflow-specific restoration)
   *
   * This method handles the complete 13-step restore process:
   * 1-2: Load and validate checkpoint
   * 3: Check version compatibility and migrate if needed
   * 4: Restore full state (handles delta chains)
   * 5: Get WorkflowGraph
   * 6-8: Create entity, restore state, and variables
   * 9-12: Create conversation, restore messages/marks, and state coordinator
   * 13: Post-restore operations (operations, fork/join, hierarchy, registry, file checkpoint)
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
    this.ensureDeps(dependencies);

    // Step 1: Load checkpoint
    const checkpoint = await dependencies.checkpointStateManager.get(checkpointId);
    if (!checkpoint) {
      throw new CheckpointNotFoundError(`Checkpoint not found`, checkpointId);
    }

    // Step 2: Validate checkpoint
    this.validateCheckpoint(checkpoint);

    // Step 3: Check version compatibility and migrate if needed
    const formatVersion = (checkpoint.metadata?.customFields?.["formatVersion"] as CheckpointFormatVersion) || CURRENT_CHECKPOINT_FORMAT_VERSION;
    if (!formatVersion) {
      logger.warn("Checkpoint missing version metadata, treating as v1.0", { checkpointId });
    }

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

    // Step 4: Restore full state (handles delta chains)
    let workflowExecutionState: WorkflowExecutionStateSnapshot;
    if (checkpoint.type === "DELTA") {
      const metadataLoader = async (entityId: string, entityType: string) => {
        const records = await dependencies.checkpointStateManager.listByEntityWithMetadata(
          entityId,
          entityType,
        );
        return records.map((r: { id: string; metadata: { previousCheckpointId?: string; checkpointType?: string; timestamp: number; chainRootId?: string; chainPosition?: number } }) => ({
          id: r.id,
          previousCheckpointId: r.metadata.previousCheckpointId,
          checkpointType: r.metadata.checkpointType as "FULL" | "DELTA",
          timestamp: r.metadata.timestamp,
          chainRootId: r.metadata.chainRootId,
          chainPosition: r.metadata.chainPosition,
        }));
      };
      
      const restorer = new BaseDeltaRestorer<Checkpoint, WorkflowExecutionStateSnapshot>(
        id => dependencies.checkpointStateManager.get(id),
        ids => dependencies.checkpointStateManager.getCheckpoints(ids),
        metadataLoader,
      );
      const restoreResult = await restorer.restore(checkpointId, checkpoint.executionId, "workflow");
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

    // Step 6-11: Build entity and restore state
    const restoreContext = this.buildEntityAndRestoreState(
      checkpoint,
      workflowExecutionState,
      graph,
      dependencies,
    );

    // Step 12-13: Post-restore operations
    await this.postRestore(restoreContext.entity, dependencies, restoreContext);

    // Publish state change event for restoration (Plan C)
    const eventBus = getExecutionEventBus();
    await eventBus.publish({
      type: "state_changed",
      executionId: restoreContext.entity.id,
      timestamp: Date.now(),
      newStatus: restoreContext.entity.getStatus(),
      changes: {
        restored: true,
        checkpointId,
      },
    });

    return {
      workflowExecutionEntity: restoreContext.entity,
      stateCoordinator: restoreContext.stateCoordinator,
      conversationManager: restoreContext.conversationManager,
    };
  }

  /**
   * Build entity and restore all state from snapshot
   */
  private buildEntityAndRestoreState(
    checkpoint: Checkpoint,
    workflowExecutionState: WorkflowExecutionStateSnapshot,
    graph: ReturnType<WorkflowGraphRegistry["get"]>,
    _dependencies: WorkflowCheckpointDependencies,
  ): WorkflowRestoreContext & { entity: WorkflowExecutionEntity } {
    // Step 6: Create entity and restore initial state
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

    // Step 11.5: Restore trigger states from checkpoint
    if (
      workflowExecutionState.triggerStates &&
      workflowExecutionState.triggerStates instanceof Map &&
      workflowExecutionState.triggerStates.size > 0
    ) {
      const triggerStateObj: Record<string, unknown> = {};
      workflowExecutionState.triggerStates.forEach((value, key) => {
        triggerStateObj[key] = value;
      });
      entity.restoreTriggerState({ triggers: Object.values(triggerStateObj) });
      logger.debug("Restored trigger states from checkpoint", {
        executionId: entity.id,
        triggerCount: Object.keys(triggerStateObj).length,
      });
    }

    // Restore hook execution context for condition evaluation after restore
    if (workflowExecutionState.hookExecutionContext) {
      entity.setHookExecutionContext(workflowExecutionState.hookExecutionContext);
      logger.debug("Restored hook execution context from checkpoint", {
        executionId: entity.id,
        hasMessages: workflowExecutionState.hookExecutionContext.messages.length > 0,
      });
    }

    // Restore FORK/JOIN aggregation state for JOIN nodes
    if (workflowExecutionState.forkJoinAggregationState) {
      entity.restoreForkJoinAggregationState(workflowExecutionState.forkJoinAggregationState);
      logger.debug("Restored FORK/JOIN aggregation state from checkpoint", {
        executionId: entity.id,
        forkNodeId: workflowExecutionState.forkJoinAggregationState.forkNodeId,
        joinNodeId: workflowExecutionState.forkJoinAggregationState.joinNodeId,
        isComplete: workflowExecutionState.forkJoinAggregationState.isAggregationComplete,
      });
    }

    return {
      entity,
      conversationManager,
      stateCoordinator,
      checkpoint,
      restoredSnapshot: workflowExecutionState,
    };
  }

  // ============================================================================
  // Abstract Method Implementations
  // ============================================================================

  /**
   * Extract state from entity for checkpoint creation
   *
   * Plan C: Now includes execution records (errors, interruptions, events)
   * that are persisted with state for disaster recovery.
   */
  protected extractState(
    entity: WorkflowExecutionEntity,
    context?: { conversationManager?: ConversationSession },
  ): WorkflowExecutionStateSnapshot {
    const workflowExecution = entity.getWorkflowExecutionData();
    const convManager = context?.conversationManager;

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

    // Plan C: Get execution records from entity's state manager
    const errorRecords = entity.state.getErrorRecords().length > 0
      ? entity.state.getErrorRecords()
      : undefined;
    const interruptionRecords = entity.state.getInterruptionRecords().length > 0
      ? entity.state.getInterruptionRecords()
      : undefined;
    const eventRecords = entity.state.getEventRecords().length > 0
      ? entity.state.getEventRecords()
      : undefined;

    // Collect complete hierarchy metadata (preferred method)
    const hierarchyMetadata = entity.getHierarchyMetadata();

    // Capture FORK/JOIN aggregation state for JOIN nodes
    const graph = typeof entity.getGraph === "function" ? entity.getGraph() : undefined;
    const currentNodeId = typeof entity.getCurrentNodeId === "function" ? entity.getCurrentNodeId() : undefined;
    const currentNode = currentNodeId && graph ? graph.getNode(currentNodeId) : undefined;
    const isJoinNode = currentNode && currentNode.type === "JOIN";
    const forkJoinAggregationState = isJoinNode
      ? entity.getForkJoinAggregationState()
      : undefined;

    // Capture hook execution context for condition evaluation after restore
    const hookExecutionContext = currentNode
      ? {
          workflowInput: entity.getInput(),
          output: entity.getOutput(),
          variables: entity.variableStateManager.getAllVariables(),
          messages: convManager?.getMessages() || [],
        }
      : undefined;

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
      hierarchy: hierarchyMetadata && hierarchyMetadata.children.length > 0
        ? hierarchyMetadata
        : undefined,
      // Plan C: Include execution records
      errorRecords,
      interruptionRecords,
      eventRecords,
      forkJoinAggregationState,
      hookExecutionContext,
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
    chainPosition?: number,
  ): Promise<Checkpoint> {
    const { getCheckpoint } = dependencies;
    const workflowDeps = dependencies as unknown as WorkflowCheckpointDependencies;

    if (checkpointType === "FULL") {
      return {
        id: checkpointId,
        executionId: entity.id,
        workflowId: entity.getWorkflowId(),
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

    // Delta checkpoint
    const previousCheckpointId = previousCheckpointIds[previousCheckpointIds.length - 1]!;
    const previousCheckpoint = await getCheckpoint(previousCheckpointId);

    if (!previousCheckpoint) {
      return {
        id: checkpointId,
        executionId: entity.id,
        workflowId: entity.getWorkflowId(),
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

    // Get previous state
    let previousState: WorkflowExecutionStateSnapshot;
    if (previousCheckpoint.type === "DELTA") {
      const metadataLoader = async (entityId: string, entityType: string) => {
        const records = await workflowDeps.checkpointStateManager.listByEntityWithMetadata(
          entityId,
          entityType,
        );
        return records.map((r: { id: string; metadata: { previousCheckpointId?: string; checkpointType?: string; timestamp: number; chainRootId?: string; chainPosition?: number } }) => ({
          id: r.id,
          previousCheckpointId: r.metadata.previousCheckpointId,
          checkpointType: r.metadata.checkpointType as "FULL" | "DELTA",
          timestamp: r.metadata.timestamp,
          chainRootId: r.metadata.chainRootId,
          chainPosition: r.metadata.chainPosition,
        }));
      };
      
      const restorer = new BaseDeltaRestorer<Checkpoint, WorkflowExecutionStateSnapshot>(
        getCheckpoint,
        ids => workflowDeps.checkpointStateManager.getCheckpoints(ids),
        metadataLoader,
      );
      const restoreResult = await restorer.restore(previousCheckpointId, entity.id, "workflow");
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
   * Extract parent ID from checkpoint
   */
  protected extractParentId(checkpoint: Checkpoint): string {
    return checkpoint.executionId;
  }

  /**
   * Create entity from restored state snapshot
   * Called by the base class restoreFromCheckpoint template.
   * Requires deps to be set via setDepsForRestore() before calling base class restore.
   */
  protected createEntityFromSnapshot(
    parentId: string,
    snapshot: WorkflowExecutionStateSnapshot,
  ): WorkflowExecutionEntity {
    if (!this.deps) {
      throw new Error("Dependencies not set. Call setDepsForRestore() before using base class restore.");
    }

    const { workflowGraphRegistry } = this.deps;

    const processedWorkflow = workflowGraphRegistry.get(
      (snapshot as unknown as { workflowId?: string }).workflowId ?? "",
    );
    if (!processedWorkflow) {
      throw new WorkflowNotFoundError(`Processed workflow not found`, "");
    }
    const graph = processedWorkflow;

    const nodeResultsArray = Object.values(snapshot.nodeResults || {});
    const workflowExecution: Partial<WorkflowExecution> = {
      id: parentId,
      workflowId: (snapshot as unknown as { workflowId?: string }).workflowId ?? "",
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

    return entity;
  }

  /**
   * Dependencies holder for createEntityFromSnapshot
   */
  private deps?: WorkflowCheckpointDependencies;

  /**
   * Set dependencies for base class template path
   */
  setDepsForRestore(deps: WorkflowCheckpointDependencies): void {
    this.deps = deps;
  }

  /**
   * Ensure deps are set for operations that need file checkpoint handling
   */
  private ensureDeps(deps: WorkflowCheckpointDependencies): void {
    this.deps = deps;
  }

  // ============================================================================
  // Post-Restore Hook
  // ============================================================================

  /**
    * Post-restore operations
    * - Restore child executions (both WORKFLOW and AGENT_LOOP)
    * - Register with registry
    * - Validate hierarchy integrity
    * - Infer FORK/JOIN completion status
    * - Restore file checkpoint
    */
   private async postRestore(
     entity: WorkflowExecutionEntity,
     dependencies: WorkflowCheckpointDependencies,
     restoreCtx: WorkflowRestoreContext,
   ): Promise<void> {
     const ctx = restoreCtx;
     const { hierarchyRegistry } = dependencies;
     const hierarchyFromSnapshot = ctx.restoredSnapshot.hierarchy;
     const childRefs = hierarchyFromSnapshot?.children ?? [];

      // Restore child executions using ChildCheckpointRestorer
      if (childRefs.length > 0) {
         logger.info("Restoring child executions for workflow", {
           executionId: entity.id,
           childCount: childRefs.length,
         });

          const restoreDeps = await this.buildChildRestoreDependencies(dependencies, hierarchyRegistry);
          const restorer = new ChildCheckpointRestorer();
          const results = await restorer.restoreChildren(
            entity as AnyExecutionEntity,
            childRefs,
            restoreDeps,
            dependencies.childCheckpointResolver,
          );

         const summary = ChildCheckpointRestorer.summarizeResults(results);
        if (summary.failed > 0) {
          logger.warn("Some child executions failed to restore", {
            executionId: entity.id,
            ...summary,
          });
        }

        // Register workflow children with workflowExecutionRegistry
        for (const result of results) {
          if (result.success && result.entity && result.childType === "WORKFLOW") {
            dependencies.workflowExecutionRegistry.register(result.entity as WorkflowExecutionEntity);
          }
        }
      }

     // Register with registry
     dependencies.workflowExecutionRegistry.register(entity);
     dependencies.workflowExecutionRegistry.registerStateCoordinator(
       entity.id,
       ctx.stateCoordinator,
     );

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
             executionId: entity.id,
             issues: validation.issues,
           });
         }
       }
     }

     // Infer FORK/JOIN completion status after child restoration
     if (entity.getGraph()) {
       const currentNode = entity.getGraph().getNode(entity.getCurrentNodeId());
       if (currentNode && currentNode.type === "JOIN") {
         const joinStatus = await this._inferForkJoinState(
           entity.getCurrentNodeId(),
           entity,
           dependencies.workflowExecutionRegistry,
           hierarchyFromSnapshot,
         );

         logger.info("Inferred JOIN completion status after child restore", {
           executionId: entity.id,
           joinNodeId: entity.getCurrentNodeId(),
           completedPaths: Array.from(joinStatus.completedPaths),
           pendingPaths: Array.from(joinStatus.pendingPaths),
           failedPaths: Array.from(joinStatus.failedPaths),
         });
       }
     }

     // Restore file checkpoint
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
         await this.handleFileCheckpointError(
           error as Error,
           "restore",
           ctx.checkpoint.executionId,
           ctx.checkpoint.id,
         );
       }
     }
   }

    /**
     * Build ChildRestoreDependencies for the current restore operation.
     * Supports both WORKFLOW and AGENT_LOOP child types via agentChildDeps.
     */
    private async buildChildRestoreDependencies(
      dependencies: WorkflowCheckpointDependencies,
      hierarchyRegistry: ExecutionHierarchyRegistry | undefined,
    ): Promise<ChildRestoreDependencies> {
      return {
        findCheckpoint: async (childId, childType) => {
          if (childType === "WORKFLOW") {
            const checkpointIds = await dependencies.checkpointStateManager.list({
              parentId: childId,
            });
            return checkpointIds.length > 0 ? checkpointIds[checkpointIds.length - 1] : undefined;
          }
          // AGENT_LOOP: use agent checkpoint storage
          const agentDeps = dependencies.agentChildDeps;
          if (agentDeps) {
            const checkpointIds = await agentDeps.agentDeps.listCheckpoints(childId);
            return checkpointIds.length > 0 ? checkpointIds[checkpointIds.length - 1] : undefined;
          }
          return undefined;
        },
        restoreEntity: async (checkpointId, childType) => {
          if (childType === "WORKFLOW") {
            const result = await this.restoreWorkflowFromCheckpoint(checkpointId, dependencies);
            return result.workflowExecutionEntity as AnyExecutionEntity;
          }
          // AGENT_LOOP: delegate to agent coordinator
          const agentChildDeps = dependencies.agentChildDeps;
          if (agentChildDeps) {
            const result = await agentChildDeps.agentCoordinator.restoreAgentLoopFromCheckpoint(
              checkpointId,
              agentChildDeps.agentDeps,
              agentChildDeps.agentRuntimeConfig,
            );
            return result as AnyExecutionEntity;
          }
          throw new Error(
            "AGENT_LOOP child restoration requires agentChildDeps in dependencies. " +
            "Use ExecutionRestoreCoordinator for cross-type restoration."
          );
        },
        registerChild: (parent, child, childRef) => {
          parent.registerChild(childRef);
          if (hierarchyRegistry) {
            hierarchyRegistry.register(child as AnyExecutionEntity);
          }
        },
        onChildRestored: undefined,
      };
    }

   // ============================================================================
   // Private Helpers
   // ============================================================================

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

    logger.debug("Workflow checkpoint error handling configured", {
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

    logger.debug("Workflow checkpoint error strategy updated", { strategy });
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
   * Get version manager for compatibility checks and migrations
   */
  getVersionManager(): CheckpointVersionManager {
    return this.versionManager;
  }

  /**
   * Check if checkpoint needs version migration
   */
  needsVersionMigration(checkpoint: Checkpoint): boolean {
    const formatVersion = (checkpoint.metadata?.customFields?.["formatVersion"] as CheckpointFormatVersion) || CURRENT_CHECKPOINT_FORMAT_VERSION;
    const compatibility = this.versionManager.checkCompatibility(formatVersion);
    return compatibility.requiresMigration;
  }

  // ============================================================================
  // Private Helpers
  // ============================================================================

  /**
   * Build metadata from options
   */
  private buildMetadata(options?: CheckpointOptions): CheckpointMetadata | undefined {
    if (!options) return undefined;

    const customFields: Record<string, unknown> = {};
    if (options.nodeId) {
      customFields['nodeId'] = options.nodeId;
    }
    if (options.toolId) {
      customFields['toolId'] = options.toolId;
    }

    return buildCheckpointMetadata({
      metadata: options.metadata,
      description: options.description ?? options.metadata?.description,
      tags: options.tags ?? options.metadata?.tags,
      customFields: Object.keys(customFields).length > 0 ? customFields : undefined,
    });
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
      getCheckpoints: ids => deps.checkpointStateManager.getCheckpoints(ids),
    };
  }

  /**
   * Handle file checkpoint error using configured error handler or fallback behavior
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

    // Fallback: use fileCheckpointManager's failureBehavior config
    const manager = this.deps?.fileCheckpointManager;
    if (!manager) return;

    const behavior = (manager as unknown as { config?: { failureBehavior?: string } }).config?.failureBehavior ?? "warn";

    if (behavior === "error") {
      throw error;
    }
    if (behavior === "warn") {
      const operationText = operation === "create" ? "creation" : "restore";
      logger.warn(`File checkpoint ${operationText} failed (non-fatal)`, {
        executionId: entityId,
        error: error.message,
      });
    }
    // "ignore" behavior: silently continue
  }



  /**
   * Infer FORK/JOIN completion status from JOIN node
   */
  private async _inferForkJoinState(
    joinNodeId: string,
    entity: WorkflowExecutionEntity,
    registry: WorkflowExecutionRegistry,
    hierarchyFromSnapshot?: import("@wf-agent/types").ExecutionHierarchyMetadata,
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
        if (hierarchyFromSnapshot) {
          const childRef = hierarchyFromSnapshot.children.find(
            (ref): ref is Extract<typeof ref, { childType: "WORKFLOW" }> => 
              ref.childType === "WORKFLOW" && ref.forkPathId === pathId,
          );
          if (childRef) {
            pendingPaths.add(pathId);
            logger.debug("FORK path found in snapshot hierarchy but not yet in registry", {
              pathId,
              childId: childRef.childId,
            });
          } else {
            pendingPaths.add(pathId);
          }
        } else {
          pendingPaths.add(pathId);
        }
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
