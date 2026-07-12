/**
 * AgentLoopEntity - Agent Loop Execution Instance
 *
 * A pure data entity that encapsulates the execution state and runtime managers of an Agent Loop.
 *
 * ## Architecture Overview
 *
 * This entity wraps three key components:
 * 1. **Config** (immutable): `AgentLoopRuntimeConfig` - defines behavior and callbacks
 * 2. **State** (mutable, serializable): `AgentLoopState` - tracks execution progress
 * 3. **Managers** (runtime): `ConversationSession`, `ExecutionHierarchyManager`, `ToolFailureProtectionState`
 *
 * ## Checkpoint Strategy
 *
 * - **Serialized**: `AgentLoopState` only (iteration history, tool calls, streaming state)
 * - **NOT Serialized**: Config (contains functions), managers (runtime-only)
 * - **On Restore**: Config is re-provided by application, state is restored from checkpoint
 *
 * ## Design Improvements
 *
 * - **Encapsulation**: conversationManager is now private, accessed via getConversationManager()
 * - **Single Responsibility**: Message operations are delegated to ConversationSession
 * - **No Dual-Write**: Messages are stored only in ConversationSession, eliminating synchronization issues
 *
 * @see AgentLoopRuntimeConfig - Runtime configuration (with callbacks)
 * @see AgentLoopState - Execution state manager (serializable)
 * @see AgentLoopFactory - Factory for creating instances
 */

import type {
  ID,
  LLMMessage,
  AgentLoopRuntimeConfig,
  AgentLoopStateSnapshot,
} from "@wf-agent/types";
import type {
  ParentExecutionContext,
  ChildExecutionReference,
  ExecutionHierarchyMetadata,
} from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";
import { getAvailableTools } from "@wf-agent/types";
import type { IExecutionEntity } from "../../shared/types/execution-entity.js";
import { AgentLoopState } from "../state-managers/agent-loop-state.js";
import { ExecutionHierarchyManager } from "../../shared/execution/execution-hierarchy-manager.js";
import type { ExecutionHierarchyRegistry } from "../../shared/registry/execution-hierarchy-registry.js";
import { createAgentInterruptionAbortReason } from "../execution/utils/index.js";
import { ToolFailureProtectionState } from "../../shared/state-managers/tool-failure-protection-state.js";
import type { ToolFailureProtectionConfig } from "../../shared/state-managers/tool-failure-protection-types.js";
import type { InterruptionState } from "../../shared/utils/interruption/interruption-state.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { TimeoutManager } from "../../shared/state-managers/timeout-manager.js";
import { TriggerStateManager } from "../../shared/triggers/trigger-state-manager.js";

const logger = createContextualLogger({ component: "AgentLoopEntity" });

/**
 * Steering Mode
 *
 * Determines how steering messages are processed:
 * - "one-at-a-time": Process one steering message at a time (default)
 * - "all": Process all steering messages at once
 */
export type SteeringMode = "one-at-a-time" | "all";

/**
 * Follow-up Mode
 *
 * Determines how follow-up messages are processed:
 * - "one-at-a-time": Process one follow-up message at a time (default)
 * - "all": Process all follow-up messages at once
 */
export type FollowUpMode = "one-at-a-time" | "all";

/**
 * AgentLoopEntity - Agent Loop Execution Instance
 *
 * Core Responsibilities:
 * - Encapsulate all data of the execution instance
 * - Provides data access interface (getter/setter).
 * - Holds the state manager instance
 * - Manages steering and follow-up queues
 *
 * Design Principles:
 * - Pure data entity: contains only data and access methods.
 * - No factory methods: handled by AgentLoopFactory.
 * - No lifecycle methods: handled by AgentLoopLifecycle
 * - Use a unified ConversationSession to manage message history.
 * - Encapsulation: conversationManager is private, accessed via getter.
 */
export class AgentLoopEntity implements IExecutionEntity {
  /** Execution Instance ID */
  readonly id: string;

  /** Discriminant property for type-safe dispatch */
  readonly instanceType = "agent" as const;

  /** Runtime configuration (immutable) */
  readonly config: AgentLoopRuntimeConfig;

  /** Execution state manager (mutable, serializable) */
  readonly state: AgentLoopState;

  /** Tool Failure Protection State Manager */
  readonly toolFailureProtection: ToolFailureProtectionState;

  /** Timeout Manager for managing execution timeouts */
  readonly timeoutManager: TimeoutManager;

  /** Trigger State Manager for tracking trigger fires and limits */
  readonly triggerStateManager: TriggerStateManager;

  /** Abort Controller */
  abortController?: AbortController;

  /** Execution Hierarchy Manager (unified parent-child relationship management) */
  private hierarchyManager: ExecutionHierarchyManager;

  /**
   * Interruption State Manager (optional, should be set by coordinator)
   * This ensures that getAbortSignal() returns the same signal used by the coordinator
   */
  private interruptionState?: InterruptionState;

  // ========== Steering & Follow-up ==========

  /** Steering message queue (for interrupting tool execution) */
  private steeringQueue: LLMMessage[] = [];

  /** Follow-up message queue (for appending after completion) */
  private followUpQueue: LLMMessage[] = [];

  /** Steering mode */
  private steeringMode: SteeringMode = "one-at-a-time";

  /** Follow-up mode */
  private followUpMode: FollowUpMode = "one-at-a-time";

  // ========== Tool Management Cache (Performance Optimization) ==========

  /** Cached available tools set */
  private cachedAvailableTools?: Set<string>;

  // ========== Warning Count (Task #5) ==========

  /** Warning count for agent execution (incremented on warnings from LLM) */
  private warningCount: number = 0;

  /** Default warning threshold (can be configured) */
  private warningThreshold: number = 10;

  /**
   * Constructor
   * @param id Execution instance ID
   * @param config Runtime configuration
   * @param state Execution state (optional, new instance created by default)
   * @param conversationManagerConfig conversation manager configuration (optional)
   * @param toolFailureProtectionConfig tool failure protection configuration (optional)
   * @param registry Optional execution hierarchy registry for cycle detection and depth calculation
   */
  constructor(
    id: string,
    config: AgentLoopRuntimeConfig,
    state?: AgentLoopState,
    toolFailureProtectionConfig?: ToolFailureProtectionConfig,
    registry?: ExecutionHierarchyRegistry,
  ) {
    this.id = id;
    this.config = config;
    this.state = state ?? new AgentLoopState();

    // Initialize hierarchy manager as root node (Agent loops are typically root executions)
    this.hierarchyManager = new ExecutionHierarchyManager(id, "AGENT_LOOP", undefined, registry);

    // Initialize tool failure protection state
    this.toolFailureProtection = new ToolFailureProtectionState(toolFailureProtectionConfig);

    // Initialize timeout manager for this execution
    this.timeoutManager = new TimeoutManager();

    // Initialize trigger state manager for tracking trigger fires
    this.triggerStateManager = new TriggerStateManager();
  }

  /**
   * Restore trigger state from checkpoint data
   * @param triggerState Serialized trigger state from checkpoint
   */
  restoreTriggerState(triggerState?: Record<string, any>): void {
    if (triggerState) {
      for (const [triggerId, state] of Object.entries(triggerState)) {
        this.triggerStateManager.setState(triggerId, state as any);
      }
    }
  }

  /**
   * Export trigger state for checkpoint
   */
  exportTriggerState(): Record<string, any> {
    return this.triggerStateManager.toJSON();
  }

  // Status Access

  /**
   * Get current state
   */
  getStatus(): AgentLoopStatus {
    return this.state.status;
  }

  /**
   * Check if it is running
   */
  isRunning(): boolean {
    return this.state.status === AgentLoopStatus.RUNNING;
  }

  /**
   * Check if it has been suspended
   */
  isPaused(): boolean {
    return this.state.status === AgentLoopStatus.PAUSED;
  }

  /**
   * Checking for completion
   */
  isCompleted(): boolean {
    return this.state.status === AgentLoopStatus.COMPLETED;
  }

  /**
   * Checking for failure
   */
  isFailed(): boolean {
    return this.state.status === AgentLoopStatus.FAILED;
  }

  /**
   * Check if it has been canceled
   */
  isCancelled(): boolean {
    return this.state.status === AgentLoopStatus.CANCELLED;
  }

  // Stop control ==========

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
   * Get the AbortSignal from the interruption state
   * Falls back to the internal abortController if interruption state is not set.
   * @returns The current AbortSignal
   */
  getAbortSignal(): AbortSignal {
    if (this.interruptionState) {
      return this.interruptionState.getAbortSignal();
    }
    if (!this.abortController) {
      this.abortController = new AbortController();
    }
    return this.abortController.signal;
  }

  /**
   * Checking if it has been aborted
   */
  get aborted(): boolean {
    return this.abortController?.signal.aborted ?? false;
  }

  /**
   * Checking if it has been aborted (alias for backward compatibility)
   */
  isAborted(): boolean {
    return this.aborted;
  }

  // ========== Interrupt Control ==========

  /**
   * suspension of action
   */
  pause(): void {
    this.state.pause();
  }

  /**
   * resumption
   */
  resume(): void {
    this.state.resume();
  }

  /**
   * stop execution
   */
  stop(): void {
    this.state.cancel();
    this.abort();
  }

  /**
   * Abort
   * @param reason reason to abort (optional)
   */
  abort(reason?: string): void {
    if (this.abortController) {
      this.abortController.abort(reason);
    }
  }

  /**
   * Check if you should pause
   *
   * Checks both AgentLoopState (via entity.interrupt()) and InterruptionState
   * (via external requestPause()) to handle both interruption paths.
   */
  shouldPause(): boolean {
    if (this.state.shouldPause()) {
      return true;
    }
    if (this.interruptionState?.shouldPause()) {
      return true;
    }
    return false;
  }

  /**
   * Check if you should stop
   *
   * Checks both AgentLoopState and InterruptionState to handle both interruption paths.
   */
  shouldStop(): boolean {
    if (this.state.shouldStop()) {
      return true;
    }
    if (this.interruptionState?.shouldStop()) {
      return true;
    }
    return false;
  }

  /**
   * Interrupt execution
   *
   * Updates both AgentLoopState (for backward-compatible shouldPause/shouldStop checks)
   * and InterruptionState (for cascade propagation, AbortSignal, and event emission).
   *
   * When InterruptionState is available, the cascade flow is:
   *   interrupt("PAUSE") → interruptState.requestPause()
   *     → sets interruptionType, aborts signal, emits EVENT_PAUSED via EventRegistry
   *     → child executions with connectToParent() auto-receive the pause
   *
   * @param type Interrupt type (PAUSE or STOP)
   */
  interrupt(type: "PAUSE" | "STOP"): void {
    this.state.interrupt(type);

    if (this.interruptionState) {
      // Delegate to InterruptionState for signal abort, event emission, and child cascade
      if (type === "PAUSE") {
        this.interruptionState.requestPause();
      } else {
        this.interruptionState.requestStop();
      }
    } else {
      // Fallback: abort own controller if no InterruptionState is configured
      const abortReason = createAgentInterruptionAbortReason(
        type,
        this.id,
        this.state.currentIteration,
      );
      this.abortController?.abort(abortReason);
    }
  }

  /**
   * Reset interrupt flag
   *
   * NOTE: This method now also calls interruptionState.resume() to trigger
   * automatic cascade propagation to all children (if interruption state is set).
   */
  resetInterrupt(): void {
    this.state.resetInterrupt();

    // Trigger resume on interruption state to auto-propagate to children
    if (this.interruptionState) {
      this.interruptionState.resume();
    }
  }

  // ========== Steering & Follow-up ==========

  /**
   * Inject a steering message
   *
   * Steering messages interrupt the agent while tools are running.
   * When detected after a tool completes:
   * 1. Remaining tools are skipped with error results
   * 2. Steering messages are injected
   * 3. LLM responds to the interruption
   *
   * @param message Steering message to inject
   */
  steer(message: LLMMessage): void {
    this.steeringQueue.push(message);
  }

  /**
   * Inject a follow-up message
   *
   * Follow-up messages are checked when there are no more tool calls
   * and no steering messages. If any are queued, they are injected
   * and another turn runs.
   *
   * @param message Follow-up message to queue
   */
  followUp(message: LLMMessage): void {
    this.followUpQueue.push(message);
  }

  /**
   * Check if there are steering messages
   */
  hasSteeringMessages(): boolean {
    return this.steeringQueue.length > 0;
  }

  /**
   * Check if there are follow-up messages
   */
  hasFollowUpMessages(): boolean {
    return this.followUpQueue.length > 0;
  }

  /**
   * Get and clear steering messages
   *
   * @returns Steering messages (based on mode)
   */
  consumeSteeringMessages(): LLMMessage[] {
    if (this.steeringQueue.length === 0) {
      return [];
    }

    if (this.steeringMode === "all") {
      const messages = [...this.steeringQueue];
      this.steeringQueue = [];
      return messages;
    } else {
      // one-at-a-time
      return [this.steeringQueue.shift()!];
    }
  }

  /**
   * Get and clear follow-up messages
   *
   * @returns Follow-up messages (based on mode)
   */
  consumeFollowUpMessages(): LLMMessage[] {
    if (this.followUpQueue.length === 0) {
      return [];
    }

    if (this.followUpMode === "all") {
      const messages = [...this.followUpQueue];
      this.followUpQueue = [];
      return messages;
    } else {
      // one-at-a-time
      return [this.followUpQueue.shift()!];
    }
  }

  /**
   * Set steering mode
   */
  setSteeringMode(mode: SteeringMode): void {
    this.steeringMode = mode;
  }

  /**
   * Set follow-up mode
   */
  setFollowUpMode(mode: FollowUpMode): void {
    this.followUpMode = mode;
  }

  /**
   * Get steering mode
   */
  getSteeringMode(): SteeringMode {
    return this.steeringMode;
  }

  /**
   * Get follow-up mode
   */
  getFollowUpMode(): FollowUpMode {
    return this.followUpMode;
  }

  /**
   * Clear steering queue
   */
  clearSteeringQueue(): void {
    this.steeringQueue = [];
  }

  /**
   * Clear follow-up queue
   */
  clearFollowUpQueue(): void {
    this.followUpQueue = [];
  }

  /**
   * Clear all queues
   */
  clearAllQueues(): void {
    this.steeringQueue = [];
    this.followUpQueue = [];
  }

  /**
   * Get steering queue length
   */
  getSteeringQueueLength(): number {
    return this.steeringQueue.length;
  }

  /**
   * Get follow-up queue length
   */
  getFollowUpQueueLength(): number {
    return this.followUpQueue.length;
  }

  // ========== Hierarchy Management ==========

  /**
   * Set parent execution context (unified API)
   *
   * Replaces parentExecutionId and nodeId fields for unified parent-child relationship management.
   * Supports both Workflow and Agent parents with type safety.
   *
   * @param parentContext Parent execution context
   * @example
   * ```typescript
   * // Set Workflow parent (Agent spawned from workflow node)
   * entity.setParentContext({
   *   parentType: 'WORKFLOW',
   *   parentId: 'parent-workflow-id'
   * });
   *
   * // Set Agent parent (Agent delegation/sub-agent)
   * entity.setParentContext({
   *   parentType: 'AGENT_LOOP',
   *   parentId: 'parent-agent-id',
   *   delegationPurpose: 'Code review task'
   * });
   * ```
   */
  setParentContext(parentContext: ParentExecutionContext): void {
    this.hierarchyManager.setParent(parentContext);
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
   * Get node ID from parent context (if parent is a workflow)
   *
   * @returns Node ID if parent is a workflow, undefined otherwise
   */
  get nodeId(): string | undefined {
    const parent = this.hierarchyManager.getParent();
    if (parent && parent.parentType === "WORKFLOW") {
      return parent.nodeId;
    }
    return undefined;
  }

  /**
   * Register child execution reference (unified API)
   *
   * Supports tracking both Workflow and Agent children spawned by this Agent.
   *
   * @param childRef Child execution reference
   * @example
   * ```typescript
   * // Register spawned Agent child (delegation)
   * entity.registerChild({
   *   childType: 'AGENT_LOOP',
   *   childId: 'sub-agent-id',
   *   nodeId: undefined, // Not applicable for agent-spawned agents
   *   spawnedAt: Date.now()
   * });
   *
   * // Register triggered Workflow child
   * entity.registerChild({
   *   childType: 'WORKFLOW',
   *   childId: 'child-workflow-id'
   * });
   * ```
   */
  registerChild(childRef: ChildExecutionReference): void {
    this.hierarchyManager.addChild(childRef);
  }

  /**
   * Unregister child execution reference (unified API)
   *
   * @param childId Child execution ID
   * @param childType Child execution type (defaults to 'AGENT_LOOP' for backward compatibility)
   */
  unregisterChild(childId: ID, childType: "WORKFLOW" | "AGENT_LOOP" = "AGENT_LOOP"): void {
    this.hierarchyManager.removeChild(childId, childType);
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
    const newManager = new ExecutionHierarchyManager(this.id, "AGENT_LOOP", metadata);
    // Replace the manager
    this.hierarchyManager = newManager;
  }

  // ========== Tool Management ==========

  /**
   * Get currently available tools from configuration
   *
   * Agent does NOT support dynamic tool changes.
   * Tools are fixed at initialization time.
   *
   * @returns Set of all available tool IDs
   */
  getAvailableTools(): Set<string> {
    // Return cached result if available
    if (this.cachedAvailableTools) {
      return this.cachedAvailableTools;
    }

    // Compute from config and cache (static, never changes)
    const configuredTools = getAvailableTools(this.config.availableTools);
    this.cachedAvailableTools = new Set(configuredTools);

    return this.cachedAvailableTools;
  }

  /**
   * Check if a specific tool is available
   *
   * @param toolId Tool ID to check
   * @returns true if tool is available
   */
  isToolAvailable(toolId: string): boolean {
    const availableTools = this.getAvailableTools();
    return availableTools.has(toolId);
  }

  // ========== Warning Count Management (Task #5) ==========

  /**
   * Get current warning count
   * @returns Current warning count
   */
  getWarningCount(): number {
    return this.warningCount;
  }

  /**
   * Increment warning count by 1
   * @returns New warning count after increment
   */
  incrementWarning(): number {
    this.warningCount++;
    return this.warningCount;
  }

  /**
   * Increment warning count by a specific amount
   * @param amount Amount to increment by (default: 1)
   * @returns New warning count after increment
   */
  addWarnings(amount: number = 1): number {
    this.warningCount += amount;
    return this.warningCount;
  }

  /**
   * Reset warning count to 0
   */
  resetWarnings(): void {
    this.warningCount = 0;
  }

  /**
   * Get warning threshold
   * @returns Current warning threshold
   */
  getWarningThreshold(): number {
    return this.warningThreshold;
  }

  /**
   * Set warning threshold
   * @param threshold New threshold value
   */
  setWarningThreshold(threshold: number): void {
    this.warningThreshold = Math.max(1, threshold); // Ensure at least 1
  }

  /**
   * Check if warning count has exceeded threshold
   * @returns true if warning count >= threshold
   */
  hasExceededWarningThreshold(): boolean {
    return this.warningCount >= this.warningThreshold;
  }

  // ========== Resource Cleanup ==========

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
        agentLoopId: this.id,
        error,
      });
    }

    // Step 2: Cleanup sub-resources with error isolation
    try {
      this.state.cleanup();
    } catch (error) {
      logger.error("Failed to cleanup state during cleanup", {
        agentLoopId: this.id,
        error,
      });
    }

    try {
      this.toolFailureProtection.cleanup();
    } catch (error) {
      logger.error("Failed to cleanup tool failure protection during cleanup", {
        agentLoopId: this.id,
        error,
      });
    }

    this.abortController = undefined;

    // Clear queues
    this.clearAllQueues();

    // Clear tool cache
    this.cachedAvailableTools = undefined;

    // Clear hierarchy manager
    this.hierarchyManager = new ExecutionHierarchyManager(this.id, "AGENT_LOOP", undefined);
  }

  /**
   * Enable await using pattern support
   * Delegates to cleanup() for resource release
   */
  async [Symbol.asyncDispose](): Promise<void> {
    this.cleanup();
  }

  // ========== Factory Methods ==========

  /**
   * Create AgentLoopEntity from state snapshot
   *
   * @param id Agent loop ID
   * @param snapshot State snapshot (contains iteration/tool call progress only)
   * @param config Runtime configuration (REQUIRED - must be re-provided, NOT from snapshot)
   * @returns Restored AgentLoopEntity
   *
   * ## Design Notes
   *
   * - `config` is a required separate parameter because `AgentLoopRuntimeConfig` contains
   *   callback functions (`transformContext`, `convertToLlm`) that cannot be serialized.
   * - The application must provide the config when restoring from checkpoint.
   * - Only `AgentLoopState` is restored from the snapshot (iteration count, tool calls, status).
   * - Messages are managed externally by `AgentStateCoordinator` and restored via checkpoint.
   *
   * @throws Error if config is not provided
   */
  static fromSnapshot(
    id: string,
    snapshot: AgentLoopStateSnapshot,
    config: AgentLoopRuntimeConfig,
  ): AgentLoopEntity {
    if (!config) {
      throw new Error(
        "AgentLoopRuntimeConfig is required to restore from snapshot. " +
          "Config contains callback functions and cannot be serialized in checkpoints. " +
          "The application must re-provide config when restoring from checkpoint.",
      );
    }

    // Create state from snapshot
    const state = new AgentLoopState();
    state.restoreFromSnapshot(snapshot);

    // Create entity with provided config and restored state
    const entity = new AgentLoopEntity(id, config, state);

    // Restore trigger state from snapshot
    if (snapshot['triggerState']) {
      entity.restoreTriggerState(snapshot['triggerState'] as Record<string, any>);
    }

    // Invalidate cache after restoration to ensure fresh computation
    entity.cachedAvailableTools = undefined;

    return entity;
  }
}
