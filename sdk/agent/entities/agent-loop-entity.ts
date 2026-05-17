/**
 * AgentLoopEntity - Agent Loop Execution Instance
 *
 * A pure data entity that encapsulates the data access operations of an Agent Loop instance.
 * Refer to WorkflowExecutionEntity design pattern.
 *
 * ## Architecture Overview
 *
 * This entity wraps three key components:
 * 1. **Config** (immutable): `AgentLoopRuntimeConfig` - defines behavior and callbacks
 * 2. **State** (mutable): `AgentLoopState` - tracks execution progress (serializable)
 * 3. **Managers** (runtime): `ConversationSession` - runtime state manager
 *
 * ## Why Not Separate Data Object?
 *
 * Unlike `WorkflowExecutionEntity` which separates data into `WorkflowExecution` + `WorkflowExecutionState`,
 * `AgentLoopEntity` keeps config and state together because:
 *
 * 1. **Simpler Execution Model**: Linear iteration vs graph traversal
 *    - Agent Loop: Single node, repeated iterations
 *    - Workflow: Multiple nodes, complex graph navigation, parallel execution
 *
 * 2. **Lighter Runtime Data**: Iteration count vs node results + variable scopes
 *    - Agent Loop: ~5-6 fields (iteration, history, status)
 *    - Workflow: ~15+ fields (graph, nodeResults, 4-level variable scopes, subgraph context)
 *
 * 3. **No Need for Separate Serializable Object**
 *    - Agent Loop: Only `AgentLoopState` needs serialization (already separate)
 *    - Workflow: Complex `WorkflowExecution` object needs independent serialization
 *
 * ## Checkpoint Strategy
 *
 * - **Serialized**: `AgentLoopState` (iteration history, tool calls, streaming state)
 * - **NOT Serialized**: `AgentLoopRuntimeConfig` (contains functions), managers (runtime-only)
 * - **On Restore**: Config is re-provided by application, state is restored from checkpoint
 *
 * ## Comparison with WorkflowExecutionEntity
 *
 * | Aspect | AgentLoopEntity | WorkflowExecutionEntity |
 * |--------|----------------|------------------------|
 * | Data Object | ❌ No separate data object | ✅ WorkflowExecution (complex graph) |
 * | State Manager | ✅ AgentLoopState | ✅ WorkflowExecutionState |
 * | Complexity | Low (linear iteration) | High (graph traversal) |
 * | Serializable Data | State only | Data object + State |
 * | Restoration | Re-provide config + restore state | Restore data object + state |
 *
 * @see AgentLoopRuntimeConfig - Runtime configuration (with callbacks)
 * @see AgentLoopState - Execution state manager (serializable)
 * @see AgentLoopFactory - Factory for creating instances
 * @see WorkflowExecutionEntity - Similar pattern for workflow execution
 */

import type { ID, LLMMessage, AgentLoopRuntimeConfig, AgentLoopStateSnapshot } from "@wf-agent/types";
import type {
  ParentExecutionContext,
  ChildExecutionReference,
  ExecutionHierarchyMetadata,
} from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";
import { AgentLoopState } from "../state-managers/agent-loop-state.js";
import {
  ConversationSession,
  type ConversationSessionConfig,
} from "../../core/messaging/conversation-session.js";
import { buildInitialMessages, type InitialMessagesConfig } from "../../core/prompt/index.js";
import { ExecutionHierarchyManager } from "../../core/execution/execution-hierarchy-manager.js";
import type { ExecutionHierarchyRegistry } from "../../core/registry/execution-hierarchy-registry.js";
import { createInterruptionAbortReason } from "../../core/utils/interruption/index.js";
import { ToolFailureProtectionState } from "../../core/state-managers/tool-failure-protection-state.js";
import type { ToolFailureProtectionConfig } from "../../core/state-managers/tool-failure-protection-types.js";

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
 */
export class AgentLoopEntity {
  /** Execution Instance ID */
  readonly id: string;

  /** deployment */
  readonly config: AgentLoopRuntimeConfig;

  /** execution status (computing) */
  readonly state: AgentLoopState;

  /** Dialogue Manager (unified message management) */
  conversationManager: ConversationSession;

  /** Tool Failure Protection State Manager (NEW) */
  readonly toolFailureProtection: ToolFailureProtectionState;

  /** Abort Controller */
  abortController?: AbortController;

  /** Parent Execution ID (if executed as a Graph node) */
  parentExecutionId?: ID;

  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;

  /** Execution Hierarchy Manager (NEW - unified parent-child relationship management) */
  private hierarchyManager: ExecutionHierarchyManager;

  // ========== Steering & Follow-up (NEW) ==========

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

  /** Flag indicating if tools have changed since last cache */
  private toolsChanged: boolean = true;

  /**
   * Constructor
   * @param id Execution instance ID
   * @param config Cyclic configuration
   * @param state Execution state (optional, new instance created by default)
   * @param conversationManagerConfig conversation manager configuration (optional)
   * @param toolFailureProtectionConfig tool failure protection configuration (optional)
   * @param registry Optional execution hierarchy registry for cycle detection and depth calculation
   */
  constructor(
    id: string,
    config: AgentLoopRuntimeConfig,
    state?: AgentLoopState,
    conversationManagerConfig?: Partial<ConversationSessionConfig>,
    toolFailureProtectionConfig?: ToolFailureProtectionConfig,
    registry?: ExecutionHierarchyRegistry,
  ) {
    this.id = id;
    this.config = config;
    this.state = state ?? new AgentLoopState();

    // Initialize hierarchy manager as root node (Agent loops are typically root executions)
    this.hierarchyManager = new ExecutionHierarchyManager(id, 'AGENT_LOOP', undefined, registry);

    // Initialize the ConversationSession (without setting an initial message).
    // The initial message will be set asynchronously in the factory method.
    this.conversationManager = new ConversationSession({
      executionId: id,
      initialMessages: [],
      ...conversationManagerConfig,
    });

    // Initialize tool failure protection state
    this.toolFailureProtection = new ToolFailureProtectionState(toolFailureProtectionConfig);
  }

  /**
   * Asynchronous initialization message (called by factory method)
   * @param config Message configuration
   */
  async initializeMessages(config: InitialMessagesConfig): Promise<void> {
    const initialMessages = await buildInitialMessages(config);
    this.conversationManager = new ConversationSession({
      executionId: this.id,
      initialMessages,
    });
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

  // ========== Message Management (delegated to ConversationSession) ==========

  /**
   * Adding a Message
   * @param message LLM message
   */
  addMessage(message: LLMMessage): void {
    this.conversationManager.addMessage(message);
  }

  /**
   * Get all messages (visible messages)
   */
  getMessages(): LLMMessage[] {
    return this.conversationManager.getMessages();
  }

  /**
   * Get all messages (including invisible messages)
   */
  getAllMessages(): LLMMessage[] {
    return this.conversationManager.getAllMessages();
  }

  /**
   * Get the most recent messages
   * @param count number of messages
   */
  getRecentMessages(count: number): LLMMessage[] {
    return this.conversationManager.getRecentMessages(count);
  }

  /**
   * Setting the message history
   * @param messages List of messages
   */
  setMessages(messages: LLMMessage[]): void {
    this.conversationManager.clear();
    for (const msg of messages) {
      this.conversationManager.addMessage(msg);
    }
  }

  /**
   * Empty message history
   */
  clearMessages(): void {
    this.conversationManager.clear();
  }

  /**
   * Normalizing Message History
   * Note: ConversationSession does not have this method and leaves it empty for compatibility.
   */
  normalizeHistory(): void {
    // No-op for compatibility
  }

  // Stop control ============

  /**
   * Getting the abort signal
   */
  getAbortSignal(): AbortSignal {
    if (!this.abortController) {
      this.abortController = new AbortController();
    }
    return this.abortController.signal;
  }

  /**
   * Checking if it has been aborted
   */
  isAborted(): boolean {
    return this.abortController?.signal.aborted ?? false;
  }

  // ========== Interrupt Control ----------

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
   */
  shouldPause(): boolean {
    return this.state.shouldPause();
  }

  /**
   * Check if you should stop
   */
  shouldStop(): boolean {
    return this.state.shouldStop();
  }

  /**
   * Interrupt execution
   * @param type Interrupt type (PAUSE or STOP)
   */
  interrupt(type: "PAUSE" | "STOP"): void {
    this.state.interrupt(type);
    
    // Create proper abort reason with interruption context
    const abortReason = createInterruptionAbortReason(type, this.id);
    this.abortController?.abort(abortReason);
  }

  /**
   * Reset interrupt flag
   */
  resetInterrupt(): void {
    this.state.resetInterrupt();
  }

  // ========== Steering & Follow-up (NEW) ==========

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

  // ========== Hierarchy Management (NEW) ==========

  /**
   * Set parent execution context (NEW unified API)
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
   * Get parent execution context (NEW unified API)
   * 
   * @returns Parent execution context or undefined if root node
   */
  getParentContext(): ParentExecutionContext | undefined {
    return this.hierarchyManager.getParent();
  }

  /**
   * Register child execution reference (NEW unified API)
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
   * Unregister child execution reference (NEW unified API)
   * 
   * @param childId Child execution ID
   * @param childType Child execution type (defaults to 'AGENT_LOOP' for backward compatibility)
   */
  unregisterChild(childId: ID, childType: 'WORKFLOW' | 'AGENT_LOOP' = 'AGENT_LOOP'): void {
    this.hierarchyManager.removeChild(childId, childType);
  }

  /**
   * Get all child execution references (NEW unified API)
   * 
   * @returns Array of child execution references
   */
  getChildReferences(): ChildExecutionReference[] {
    return this.hierarchyManager.getChildren();
  }

  /**
   * Get hierarchy depth (NEW unified API)
   * 
   * @returns Depth in hierarchy tree (0 for root)
   */
  getHierarchyDepth(): number {
    return this.hierarchyManager.getDepth();
  }

  /**
   * Get root execution ID (NEW unified API)
   * 
   * @returns Root execution ID
   */
  getRootExecutionId(): ID {
    return this.hierarchyManager.getRootExecutionId();
  }

  /**
   * Get root execution type (NEW unified API)
   * 
   * @returns Root execution type
   */
  getRootExecutionType(): 'WORKFLOW' | 'AGENT_LOOP' {
    return this.hierarchyManager.getRootExecutionType();
  }

  /**
   * Check if this is a root execution (NEW unified API)
   * 
   * @returns True if no parent
   */
  isRootExecution(): boolean {
    return this.hierarchyManager.getParent() === undefined;
  }

  /**
   * Get complete hierarchy metadata (NEW unified API)
   * 
   * @returns Hierarchy metadata for serialization
   */
  getHierarchyMetadata(): ExecutionHierarchyMetadata | undefined {
    return this.hierarchyManager.toMetadata();
  }

  /**
   * Restore hierarchy from metadata (NEW unified API)
   * 
   * Used during checkpoint restoration.
   * 
   * @param metadata Hierarchy metadata
   */
  restoreHierarchy(metadata: ExecutionHierarchyMetadata): void {
    // Create new manager with restored metadata
    const newManager = new ExecutionHierarchyManager(
      this.id,
      'AGENT_LOOP',
      metadata
    );
    // Replace the manager
    this.hierarchyManager = newManager;
  }

  // ========== Tool Management (NEW) ==========

  /**
   * Dynamically add tools during execution
   * 
   * Updates the availableTools.dynamic set in the config.
   * New tools will be available in the next iteration.
   * 
   * @param toolIds Array of tool IDs to add
   * @param overwrite Whether to overwrite existing tools (default: false)
   * @returns Number of tools actually added
   */
  addTools(toolIds: string[], overwrite: boolean = false): number {
    if (!this.config.availableTools) {
      this.config.availableTools = { initial: [] };
    }
    
    if (!this.config.availableTools.dynamic) {
      this.config.availableTools.dynamic = new Set();
    }
    
    let addedCount = 0;
    for (const toolId of toolIds) {
      if (overwrite || !this.config.availableTools.dynamic.has(toolId)) {
        this.config.availableTools.dynamic.add(toolId);
        addedCount++;
      }
    }
    
    // Invalidate cache when tools change
    if (addedCount > 0) {
      this.toolsChanged = true;
    }
    
    return addedCount;
  }

  /**
   * Remove dynamically added tools
   * 
   * Only removes tools from the dynamic set.
   * Cannot remove tools from the initial set.
   * 
   * @param toolIds Array of tool IDs to remove
   * @returns Number of tools actually removed
   */
  removeTools(toolIds: string[]): number {
    if (!this.config.availableTools?.dynamic) {
      return 0;
    }
    
    let removedCount = 0;
    for (const toolId of toolIds) {
      if (this.config.availableTools.dynamic.delete(toolId)) {
        removedCount++;
      }
    }
    
    // Invalidate cache when tools change
    if (removedCount > 0) {
      this.toolsChanged = true;
    }
    
    return removedCount;
  }

  /**
   * Get currently available tools (initial + dynamic)
   * 
   * Uses caching to avoid recomputing on every call.
   * Cache is invalidated when tools are added or removed.
   * 
   * @returns Set of all available tool IDs
   */
  getAvailableTools(): Set<string> {
    // Return cached result if available and not stale
    if (this.cachedAvailableTools && !this.toolsChanged) {
      return this.cachedAvailableTools;
    }
    
    // Recompute and cache
    const initial = this.config.availableTools?.initial || [];
    const dynamic = Array.from(this.config.availableTools?.dynamic || []);
    this.cachedAvailableTools = new Set([...initial, ...dynamic]);
    this.toolsChanged = false;
    
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

  // ========== Resource Cleanup ==========

  /**
   * Cleanup all resources held by this entity
   * This method should be called when the entity is no longer needed.
   */
  cleanup(): void {
    // Cleanup state
    this.state.cleanup();

    // Cleanup conversation manager
    this.conversationManager.cleanup();

    // Cleanup tool failure protection state
    this.toolFailureProtection.cleanup();

    // Clear abort controller
    this.abortController = undefined;

    // Clear queues
    this.clearAllQueues();

    // Clear tool cache
    this.cachedAvailableTools = undefined;
    this.toolsChanged = true;
  }

  // ========== Factory Methods ==========

  /**
   * Create AgentLoopEntity from state snapshot
   * @param id Agent loop ID
   * @param snapshot State snapshot
   * @returns Restored AgentLoopEntity
   */
  static fromSnapshot(id: string, snapshot: AgentLoopStateSnapshot): AgentLoopEntity {
    // Create state from snapshot
    const state = new AgentLoopState();
    state.restoreFromSnapshot(snapshot);

    // Restore config from snapshot (may contain functions, so cast is needed)
    const config = snapshot.config as AgentLoopRuntimeConfig;

    // 【新增】Restore dynamic tools from snapshot
    const dynamicTools = snapshot['dynamicTools'] as string[] | undefined;
    if (dynamicTools && dynamicTools.length > 0) {
      if (!config.availableTools) {
        config.availableTools = { initial: [] };
      }
      config.availableTools.dynamic = new Set(dynamicTools);
    }

    // Create entity with restored state and config
    const entity = new AgentLoopEntity(id, config, state);

    // Restore messages
    entity.setMessages(snapshot.messages as LLMMessage[]);

    // Invalidate cache after restoration to ensure fresh computation
    entity.toolsChanged = true;
    entity.cachedAvailableTools = undefined;

    return entity;
  }
}
