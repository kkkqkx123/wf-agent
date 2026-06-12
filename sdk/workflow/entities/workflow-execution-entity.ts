/**
 * WorkflowExecutionEntity - A pure data entity that encapsulates the data access operations for WorkflowExecution instances.
 * Refer to the design pattern of AgentLoopEntity.
 *
 * Architecture Design (Single Data Source for Messages):
 * - All message operations: Managed by WorkflowStateCoordinator using ConversationSession
 * - Parent-child message passing: Use WorkflowStateCoordinator.exportMessagesForChild/importMessagesFromChild
 *
 * Notes:
 * - The factory method is provided by WorkflowExecutionBuilder.
 * - Lifecycle management is handled by WorkflowLifecycleCoordinator and WorkflowLifecycleCoordinator.
 * - The snapshot functionality is provided by WorkflowExecutionSnapshotManager.
 * - ConversationSession management is handled by WorkflowStateCoordinator (not held directly).
 *
 */

import type { ID, NodeExecutionResult, BaseEvent } from "@wf-agent/types";
import type {
  WorkflowExecution,
  WorkflowExecutionStatus,
  WorkflowExecutionType,
} from "@wf-agent/types";
import type { WorkflowGraph } from "../types/graph/preprocessed-graph.js";
import type {
  ParentExecutionContext,
  ChildExecutionReference,
  ExecutionHierarchyMetadata,
} from "@wf-agent/types";
import type { SubgraphContext } from "../state-managers/execution-state.js";
import { ExecutionState } from "../state-managers/execution-state.js";
import { WorkflowExecutionState } from "../state-managers/workflow-execution-state.js";
import { VariableManager } from "../state-managers/variable-manager.js";
import { ExecutionHierarchyManager } from "../../core/execution/execution-hierarchy-manager.js";
import type { ExecutionHierarchyRegistry } from "../../core/registry/execution-hierarchy-registry.js";
import { ToolFailureProtectionState } from "../../core/state-managers/tool-failure-protection-state.js";
import type { ToolFailureProtectionConfig } from "../../core/state-managers/tool-failure-protection-types.js";
import type { InterruptionState } from "../../core/utils/interruption/interruption-state.js";
import { createWorkflowInterruptionAbortReason } from "../execution/utils/workflow-interruption-utils.js";
import type { EventRegistry } from "../../core/registry/event-registry.js";
import type { Abortable } from "../../core/types/abortable.js";
import { DependencyManager } from "../evaluation/index.js";
import { SyncBarrier } from "../execution/barriers/sync-barrier.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "WorkflowExecutionEntity" });

/**
 * WorkflowExecutionEntity - Workflow Execution Entity
 *
 * Core Responsibilities:
 * - Encapsulates all data of the execution instance
 * - Provides data access interfaces (getter/setter)
 * - Holds an instance of the state manager
 *
 * Design Principles:
 * - Pure data entity: Contains only data and access methods
 * - Does not include factory methods: This is the responsibility of WorkflowExecutionBuilder
 * - Does not include lifecycle methods: This is the responsibility of WorkflowLifecycleCoordinator/Coordinator
 * - ConversationSession is managed by WorkflowStateCoordinator, not held directly
 *
 * Migration Note:
 * - conversationManager has been removed from WorkflowExecutionEntity
 * - Use WorkflowStateCoordinator for unified state management
 * - This eliminates data redundancy and synchronization issues
 */
export class WorkflowExecutionEntity implements Abortable {
  /** Execute instance ID */
  readonly id: string;

  /** WorkflowExecution data object (private, not exposed) */
  private readonly workflowExecution: WorkflowExecution;

  /** Execution Status */
  readonly state: WorkflowExecutionState;

  /** Execution Status Manager (Subgraph Execution Stack) */
  private readonly executionState: ExecutionState;

  /** Variable State Manager (NEW - Simplified) */
  readonly variableStateManager: VariableManager;

  /** Tool Failure Protection State Manager (NEW) */
  readonly toolFailureProtection: ToolFailureProtectionState;

  /** Stop Controller */
  abortController?: AbortController;

  /** Trigger Management */
  triggerManager?: unknown;

  /** Execution Hierarchy Manager (unified parent-child relationship management) */
  private hierarchyManager: ExecutionHierarchyManager;

  /** Execution Hierarchy Registry reference (for SyncBarrier initialization) */
  private readonly hierarchyRegistry?: ExecutionHierarchyRegistry;

  /**
   * Interruption State Manager (optional, should be set by coordinator)
   * This ensures that getAbortSignal() returns the same signal used by the coordinator
   */
  private interruptionState?: InterruptionState;

  /**
   * Sync Barrier for fork/join synchronization (only for parent executions with FORK nodes)
   * Manages path-to-execution mappings and provides waiting mechanisms
   */
  private syncBarrier?: SyncBarrier;

  private _depManager?: DependencyManager;

  /**
   * Constructor
   * @param workflowExecution: WorkflowExecution data object
   * @param executionState: Execution state manager
   * @param state: Execution status (optional; a new instance is created by default)
   * @param toolFailureProtectionConfig: Tool failure protection configuration (optional)
   * @param registry: Optional execution hierarchy registry for cycle detection and depth calculation
   */
  constructor(
    workflowExecution: WorkflowExecution,
    executionState: ExecutionState,
    state?: WorkflowExecutionState,
    toolFailureProtectionConfig?: ToolFailureProtectionConfig,
    registry?: ExecutionHierarchyRegistry,
  ) {
    this.id = workflowExecution.id;
    this.workflowExecution = workflowExecution;
    this.executionState = executionState;
    this.state = state ?? new WorkflowExecutionState();
    this.variableStateManager = new VariableManager();

    // Set execution entity reference for runtime validation
    this.variableStateManager.setExecutionEntity(this);

    // Initialize tool failure protection state
    this.toolFailureProtection = new ToolFailureProtectionState(toolFailureProtectionConfig);

    // Initialize hierarchy manager with existing hierarchy metadata or as root node
    this.hierarchyManager = new ExecutionHierarchyManager(
      workflowExecution.id,
      "WORKFLOW",
      workflowExecution.hierarchy,
      registry,
    );

    // Store registry reference for SyncBarrier initialization
    this.hierarchyRegistry = registry;
  }

  // Basic Property Access ============

  /**
   * Get execution ID
   * @returns Execution ID
   */
  getExecutionId(): string {
    return this.id;
  }

  getWorkflowId(): string {
    return this.workflowExecution.workflowId;
  }

  getWorkflowVersion(): string {
    return this.workflowExecution.workflowVersion || "unknown";
  }

  getStatus(): WorkflowExecutionStatus {
    return this.state.status;
  }

  setStatus(status: WorkflowExecutionStatus): void {
    this.state.status = status;
  }

  getExecutionType(): WorkflowExecutionType {
    return this.workflowExecution.executionType || "MAIN";
  }

  setExecutionType(executionType: WorkflowExecutionType): void {
    this.workflowExecution.executionType = executionType;
  }

  getCurrentNodeId(): string {
    return this.workflowExecution.currentNodeId;
  }

  setCurrentNodeId(nodeId: string): void {
    this.workflowExecution.currentNodeId = nodeId;
  }

  getInput(): Record<string, unknown> {
    return this.workflowExecution.input;
  }

  getOutput(): Record<string, unknown> {
    return this.workflowExecution.output;
  }

  setOutput(output: Record<string, unknown>): void {
    this.workflowExecution.output = output;
  }

  // Execution Results ============

  addNodeResult(result: NodeExecutionResult): void {
    this.workflowExecution.nodeResults.push(result);
  }

  getNodeResults(): NodeExecutionResult[] {
    return this.workflowExecution.nodeResults as NodeExecutionResult[];
  }

  // Error Message

  getErrors(): unknown[] {
    return this.workflowExecution.errors;
  }

  // ========== Time Information ----------

  getStartTime(): number | null {
    return this.state.startTime;
  }

  getEndTime(): number | null {
    return this.state.endTime;
  }

  // ========== Graph Navigation ----------

  getGraph(): WorkflowGraph {
    return this.workflowExecution.graph as unknown as WorkflowGraph;
  }

  // ========== Subgraph Execution Status ----------

  enterSubgraph(workflowId: ID, parentWorkflowId: ID, input: unknown): void {
    this.executionState.enterSubgraph(workflowId, parentWorkflowId, input);
  }

  exitSubgraph(): void {
    this.executionState.exitSubgraph();
  }

  getCurrentSubgraphContext(): SubgraphContext | null {
    return this.executionState.getCurrentSubgraphContext();
  }

  getSubgraphStack(): SubgraphContext[] {
    return this.executionState.getSubgraphStack();
  }

  // ========== Fork/Join Context ----------

  setForkId(forkId: string): void {
    if (!this.workflowExecution.forkJoinContext) {
      this.workflowExecution.forkJoinContext = { forkId, forkPathId: "" };
    }
    this.workflowExecution.forkJoinContext.forkId = forkId;
  }

  setForkPathId(forkPathId: string): void {
    if (!this.workflowExecution.forkJoinContext) {
      this.workflowExecution.forkJoinContext = { forkId: "", forkPathId };
    }
    this.workflowExecution.forkJoinContext.forkPathId = forkPathId;
  }

  // Triggering Sub-workflow Context ============

  getChildExecutionIds(): ID[] {
    return this.hierarchyManager
      .getChildren()
      .filter(ref => ref.childType === "WORKFLOW")
      .map(ref => ref.childId);
  }

  getTriggeredSubworkflowId(): ID | undefined {
    return this.workflowExecution.triggeredSubworkflowContext?.triggeredSubworkflowId;
  }

  setTriggeredSubworkflowId(subworkflowId: ID): void {
    if (!this.workflowExecution.triggeredSubworkflowContext) {
      this.workflowExecution.triggeredSubworkflowContext = {
        parentExecutionId: "",
        childExecutionIds: [],
        triggeredSubworkflowId: subworkflowId,
      };
    }
    this.workflowExecution.triggeredSubworkflowContext.triggeredSubworkflowId = subworkflowId;
  }

  // Variable Management ============

  /**
   * Get the variable
   * @param name Variable name
   */
  getVariable(name: string): unknown {
    return this.variableStateManager.getVariable(name);
  }

  /**
   * Set a variable
   * @param name: Variable name
   * @param value: Variable value
   */
  setVariable(name: string, value: unknown): void {
    this.variableStateManager.setVariable(name, value);
  }

  /**
   * Get all variables
   */
  getAllVariables(): Record<string, unknown> {
    return this.variableStateManager.getAllVariables();
  }

  /**
   * Delete the variable
   * @param name Variable name
   */
  deleteVariable(name: string): boolean {
    return this.variableStateManager.deleteVariable(name);
  }

  // Stop control ============

  /**
   * Set the interruption state manager (called by coordinator after creation)
   * @param interruptionState The InterruptionState instance to use
   */
  setInterruptionState(interruptionState: InterruptionState): void {
    this.interruptionState = interruptionState;
  }

  /**
   * Get the interruption state manager
   * @returns The InterruptionState instance or undefined if not set
   */
  getInterruptionState(): InterruptionState | undefined {
    return this.interruptionState;
  }

  /**
   * Get the abort signal
   * This method is kept for backward compatibility with hooks and handlers
   */
  getAbortSignal(): AbortSignal {
    // If interruptionState is set, use it (preferred)
    if (this.interruptionState) {
      return this.interruptionState.getAbortSignal();
    }

    // Fallback to legacy abortController for backward compatibility
    if (!this.abortController) {
      this.abortController = new AbortController();
    }
    return this.abortController.signal;
  }

  /**
   * Abort execution
   * @param reason Optional reason for abortion
   */
  abort(reason?: string): void {
    if (!this.abortController) {
      this.abortController = new AbortController();
    }
    this.abortController.abort(reason);
    this.state.status = "CANCELLED";
  }

  /**
   * Whether this execution has been aborted
   */
  get aborted(): boolean {
    return this.abortController?.signal.aborted ?? false;
  }

  // Child AgentLoop Management ============

  /**
   * Register child AgentLoop ID
   * @param agentLoopId AgentLoop ID
   */
  registerChildAgentLoop(agentLoopId: string): void {
    this.registerChild({
      childType: "AGENT_LOOP",
      childId: agentLoopId,
      createdAt: Date.now(),
    });
  }

  /**
   * Unregister child AgentLoop ID
   * @param agentLoopId AgentLoop ID
   */
  unregisterChildAgentLoop(agentLoopId: string): void {
    this.unregisterChild(agentLoopId, "AGENT_LOOP");
  }

  /**
   * Get all child AgentLoop IDs
   * @returns Set of AgentLoop IDs
   */
  getChildAgentLoopIds(): Set<string> {
    const children = this.hierarchyManager
      .getChildren()
      .filter(ref => ref.childType === "AGENT_LOOP")
      .map(ref => ref.childId);
    return new Set(children);
  }

  /**
   * Check if has child AgentLoops
   * @returns Whether there are child AgentLoops
   */
  hasChildAgentLoops(): boolean {
    return this.hierarchyManager.getChildren().some(ref => ref.childType === "AGENT_LOOP");
  }

  // ========== Data Access (for internal use) ----------

  /**
   * Get the raw WorkflowExecution data object (alias for getWorkflowExecutionData for compatibility)
   * @returns WorkflowExecution data object
   */
  getExecution(): WorkflowExecution {
    return this.workflowExecution;
  }

  /**
   * Get the raw WorkflowExecution data object
   * @returns WorkflowExecution data object
   * @internal For internal use only
   */
  getWorkflowExecutionData(): WorkflowExecution {
    return this.workflowExecution;
  }

  /**
   * Get the execution state
   * @returns Execution state
   * @internal For internal use only
   */
  getExecutionState(): ExecutionState {
    return this.executionState;
  }

  /**
   * Get execution time in milliseconds
   * @returns Execution time or 0 if not completed
   */
  getExecutionTime(): number {
    if (this.state.startTime && this.state.endTime) {
      return this.state.endTime - this.state.startTime;
    }
    return 0;
  }

  // Trigger State Management ============

  /**
   * Get trigger state snapshot
   * @returns Trigger state snapshot with triggers array
   */
  getTriggerStateSnapshot(): { triggers: unknown[] } {
    // If triggerManager is not set or doesn't have getTriggers method, return empty array
    if (
      !this.triggerManager ||
      typeof (this.triggerManager as { getTriggers?: () => unknown[] }).getTriggers !== "function"
    ) {
      return { triggers: [] };
    }
    return {
      triggers: (this.triggerManager as { getTriggers: () => unknown[] }).getTriggers(),
    };
  }

  /**
   * Restore trigger state from snapshot
   * @param snapshot Trigger state snapshot with triggers array
   */
  restoreTriggerState(snapshot: { triggers: unknown[] }): void {
    // If triggerManager is not set or doesn't have restoreTriggers method, do nothing
    if (
      !this.triggerManager ||
      typeof (this.triggerManager as { restoreTriggers?: (triggers: unknown[]) => void })
        .restoreTriggers !== "function"
    ) {
      return;
    }
    (this.triggerManager as { restoreTriggers: (triggers: unknown[]) => void }).restoreTriggers(
      snapshot.triggers,
    );
  }

  // Lifecycle Control Methods ============

  /**
   * Pause the workflow execution
   */
  pause(): void {
    this.state.status = "PAUSED";
  }

  /**
   * Resume the workflow execution
   */
  resume(): void {
    this.state.status = "RUNNING";
  }

  /**
   * Stop the workflow execution
   */
  stop(): void {
    this.state.status = "STOPPED";
    if (this.abortController) {
      this.abortController.abort();
    }
  }

  /**
   * Interrupt the workflow execution
   * @param type Interrupt type (PAUSE or STOP)
   */
  interrupt(type: "PAUSE" | "STOP"): void {
    this.state.interrupt(type);

    // Create proper abort reason with workflow context (nodeId)
    const currentNodeId = this.getCurrentNodeId();
    if (currentNodeId && this.abortController) {
      const abortReason = createWorkflowInterruptionAbortReason(type, this.id, currentNodeId);
      this.abortController.abort(abortReason);
    }
  }

  /**
   * Reset the interrupt flag
   *
   * NOTE: This method now also calls interruptionState.resume() to trigger
   * automatic cascade propagation to all children (if interruption state is set).
   */
  resetInterrupt(): void {
    this.state.interrupted = false;

    // Trigger resume on interruption state to auto-propagate to children
    if (this.interruptionState) {
      this.interruptionState.resume();
    }
  }

  // Parent-Child Execution Management ============

  /**
   * Synchronizes hierarchy metadata from manager to data object
   *
   * This is a short-term solution to reduce code duplication while maintaining
   * backward compatibility with the old workflowExecution.hierarchy field.
   *
   * @private
   */
  private syncHierarchyToDataObject(): void {
    const metadata = this.hierarchyManager.toMetadata();

    if (!this.workflowExecution.hierarchy) {
      this.workflowExecution.hierarchy = { ...metadata };
    } else {
      // Update existing hierarchy object
      this.workflowExecution.hierarchy.parent = metadata.parent;
      this.workflowExecution.hierarchy.children = metadata.children;
      this.workflowExecution.hierarchy.depth = metadata.depth;
      this.workflowExecution.hierarchy.rootExecutionId = metadata.rootExecutionId;
      this.workflowExecution.hierarchy.rootExecutionType = metadata.rootExecutionType;
    }
  }

  /**
   * Set parent execution context (unified API)
   *
   * Supports both Workflow and Agent parents with type safety.
   * Provides automatic validation including cycle detection and depth limit enforcement.
   *
   * @param parentContext Parent execution context
   * @example
   * ```typescript
   * // Set Workflow parent
   * entity.setParentContext({
   *   parentType: 'WORKFLOW',
   *   parentId: 'parent-workflow-id'
   * });
   *
   * // Set Agent parent with delegation purpose
   * entity.setParentContext({
   *   parentType: 'AGENT_LOOP',
   *   parentId: 'parent-agent-id',
   *   delegationPurpose: 'Execute subtask'
   * });
   * ```
   */
  setParentContext(parentContext: ParentExecutionContext): void {
    this.hierarchyManager.setParent(parentContext);

    // Sync to data object for backward compatibility
    this.syncHierarchyToDataObject();
  }

  /**
   * Get parent execution context (unified API)
   *
   * @returns Parent execution context or undefined if root node
   */
  getParentContext(): ParentExecutionContext | undefined {
    return this.hierarchyManager.getParent();
  }

  /**
   * Register child execution reference (unified API)
   *
   * Supports both Workflow and Agent children with type safety.
   * Tracks creation timestamp and supports optional metadata.
   *
   * @param childRef Child execution reference
   * @example
   * ```typescript
   * // Register Workflow child
   * entity.registerChild({
   *   childType: 'WORKFLOW',
   *   childId: 'child-workflow-id'
   * });
   *
   * // Register Agent child with metadata
   * entity.registerChild({
   *   childType: 'AGENT_LOOP',
   *   childId: 'child-agent-id',
   *   nodeId: 'agent-node-id',
   *   spawnedAt: Date.now()
   * });
   * ```
   */
  registerChild(childRef: ChildExecutionReference): void {
    this.hierarchyManager.addChild(childRef);

    // Sync to data object for backward compatibility
    this.syncHierarchyToDataObject();
  }

  /**
   * Unregister child execution reference (unified API)
   *
   * @param childId Child execution ID
   * @param childType Child execution type (defaults to 'WORKFLOW' for backward compatibility)
   */
  unregisterChild(childId: ID, childType: "WORKFLOW" | "AGENT_LOOP" = "WORKFLOW"): void {
    this.hierarchyManager.removeChild(childId, childType);

    // Sync to data object for backward compatibility
    this.syncHierarchyToDataObject();
  }

  /**
   * Get all child execution references (unified API)
   *
   * @returns Array of child execution references
   */
  getChildReferences(): ChildExecutionReference[] {
    return this.hierarchyManager.getChildren();
  }

  /**
   * Get hierarchy depth (unified API)
   *
   * @returns Depth in hierarchy tree (0 for root)
   */
  getHierarchyDepth(): number {
    return this.hierarchyManager.getDepth();
  }

  /**
   * Get root execution ID (unified API)
   *
   * @returns Root execution ID
   */
  getRootExecutionId(): ID {
    return this.hierarchyManager.getRootExecutionId();
  }

  /**
   * Get root execution type (unified API)
   *
   * @returns Root execution type
   */
  getRootExecutionType(): "WORKFLOW" | "AGENT_LOOP" {
    return this.hierarchyManager.getRootExecutionType();
  }

  /**
   * Check if this is a root execution (unified API)
   *
   * @returns True if no parent
   */
  isRootExecution(): boolean {
    return this.hierarchyManager.getParent() === undefined;
  }

  /**
   * Get complete hierarchy metadata (unified API)
   *
   * @returns Hierarchy metadata for serialization
   */
  getHierarchyMetadata(): ExecutionHierarchyMetadata | undefined {
    return this.hierarchyManager.toMetadata();
  }

  /**
   * Restore hierarchy from metadata (unified API)
   *
   * Used during checkpoint restoration.
   *
   * @param metadata Hierarchy metadata
   */
  restoreHierarchy(metadata: ExecutionHierarchyMetadata): void {
    // Create new manager with restored metadata
    const newManager = new ExecutionHierarchyManager(this.id, "WORKFLOW", metadata);
    // Replace the manager
    this.hierarchyManager = newManager;

    // Also update the data object
    this.workflowExecution.hierarchy = metadata;
  }

  // Event Building ============

  /**
   * Build an event using the provided builder function
   * @param builder Event builder function
   * @param params Additional parameters for the event builder
   * @returns Built event object
   */
  buildEvent<T extends BaseEvent, P extends Record<string, unknown>>(
    builder: (params: P) => T,
    params?: Omit<P, "executionId" | "workflowId">,
  ): T {
    const fullParams = {
      executionId: this.id,
      workflowId: this.workflowExecution.workflowId,
      ...params,
    } as unknown as P;
    return builder(fullParams);
  }

  /**
   * Cleanup all resources held by this entity
   * This method should be called when the entity is no longer needed.
   */
  cleanup(): void {
    // Step 1: Dispose interruption state (cleans up all listeners and child references)
    try {
      this.interruptionState?.dispose();
    } catch (error) {
      logger.error("Failed to dispose interruption state during cleanup", {
        executionId: this.id,
        error,
      });
    }

    // Step 2: Clear sync barrier
    try {
      this.syncBarrier?.clear();
    } catch (error) {
      logger.error("Failed to clear sync barrier during cleanup", {
        executionId: this.id,
        error,
      });
    }

    // Step 3: Cleanup sub-resources with error isolation
    try {
      this.state.cleanup();
    } catch (error) {
      logger.error("Failed to cleanup state during cleanup", {
        executionId: this.id,
        error,
      });
    }

    try {
      this.variableStateManager.cleanup();
    } catch (error) {
      logger.error("Failed to cleanup variable state manager during cleanup", {
        executionId: this.id,
        error,
      });
    }

    try {
      this.toolFailureProtection.cleanup();
    } catch (error) {
      logger.error("Failed to cleanup tool failure protection during cleanup", {
        executionId: this.id,
        error,
      });
    }

    this.abortController = undefined;

    // Clear hierarchy manager
    this.hierarchyManager = new ExecutionHierarchyManager(this.id, "WORKFLOW", undefined);
  }

  /**
   * Enable await using pattern support
   * Delegates to cleanup() for resource release
   */
  async [Symbol.asyncDispose](): Promise<void> {
    this.cleanup();
  }

  // ========== Sync Barrier Management ==========

  /**
   * Initialize SyncBarrier for this execution
   * Should be called when a FORK node is encountered in the workflow
   *
   * @param eventRegistry Event registry for cross-execution event listening
   */
  initializeSyncBarrier(eventRegistry: EventRegistry): void {
    if (!this.syncBarrier) {
      // Pass the execution hierarchy registry to SyncBarrier
      // This allows SyncBarrier to look up execution entities by ID
      this.syncBarrier = new SyncBarrier(this.id, eventRegistry, this.hierarchyRegistry);
      logger.debug("SyncBarrier initialized for execution", {
        executionId: this.id,
      });
    }
  }

  /**
   * Get the DependencyManager instance for caching compiled expressions
   * across route conditions, loop break conditions, etc.
   * Lazily initialized on first access.
   *
   * @returns DependencyManager instance
   */
  getDepManager(): DependencyManager {
    if (!this._depManager) {
      this._depManager = new DependencyManager();
    }
    return this._depManager;
  }

  /**
   * Get the SyncBarrier instance
   * Returns undefined if not initialized (no FORK nodes in workflow)
   *
   * @returns SyncBarrier instance or undefined
   */
  getSyncBarrier(): SyncBarrier | undefined {
    return this.syncBarrier;
  }

  /**
   * Check if SyncBarrier is initialized
   *
   * @returns True if SyncBarrier exists
   */
  hasSyncBarrier(): boolean {
    return this.syncBarrier !== undefined;
  }
}
