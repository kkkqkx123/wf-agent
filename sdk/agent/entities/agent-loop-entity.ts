/**
 * AgentLoopEntity - Agent Loop Execution Instance
 *
 * A pure data entity that encapsulates the data access operations of an Agent Loop instance.
 * Refer to WorkflowExecutionEntity design pattern.
 *
 * ## Architecture Overview
 *
 * This entity wraps three key components:
 * 1. **Config** (immutable): `AgentLoopConfig` - defines behavior and callbacks
 * 2. **State** (mutable): `AgentLoopState` - tracks execution progress (serializable)
 * 3. **Managers** (runtime): `ConversationSession`, `VariableState` - runtime state managers
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
 * - **NOT Serialized**: `AgentLoopConfig` (contains functions), managers (runtime-only)
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
 * @see AgentLoopConfig - Runtime configuration (with callbacks)
 * @see AgentLoopState - Execution state manager (serializable)
 * @see AgentLoopFactory - Factory for creating instances
 * @see WorkflowExecutionEntity - Similar pattern for workflow execution
 */

import type { ID, LLMMessage, AgentLoopConfig, AgentLoopStateSnapshot } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";
import { AgentLoopState } from "./agent-loop-state.js";
import {
  ConversationSession,
  type ConversationSessionConfig,
} from "../../core/messaging/conversation-session.js";
import { VariableState } from "../../workflow/state-managers/variable-state.js";
import { buildInitialMessages, type InitialMessagesConfig } from "../../core/prompt/index.js";

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
  readonly config: AgentLoopConfig;

  /** execution status (computing) */
  readonly state: AgentLoopState;

  /** Dialogue Manager (unified message management) */
  conversationManager: ConversationSession;

  /** Variable Status Manager */
  readonly variableStateManager: VariableState;

  /** Abort Controller */
  abortController?: AbortController;

  /** Parent Execution ID (if executed as a Graph node) */
  parentExecutionId?: ID;

  /** Node ID (if executed as a Graph node) */
  nodeId?: ID;

  // ========== Steering & Follow-up (NEW) ==========

  /** Steering message queue (for interrupting tool execution) */
  private steeringQueue: LLMMessage[] = [];

  /** Follow-up message queue (for appending after completion) */
  private followUpQueue: LLMMessage[] = [];

  /** Steering mode */
  private steeringMode: SteeringMode = "one-at-a-time";

  /** Follow-up mode */
  private followUpMode: FollowUpMode = "one-at-a-time";

  /**
   * Constructor
   * @param id Execution instance ID
   * @param config Cyclic configuration
   * @param state Execution state (optional, new instance created by default)
   * @param conversationManagerConfig conversation manager configuration (optional)
   */
  constructor(
    id: string,
    config: AgentLoopConfig,
    state?: AgentLoopState,
    conversationManagerConfig?: Partial<ConversationSessionConfig>,
  ) {
    this.id = id;
    this.config = config;
    this.state = state ?? new AgentLoopState();
    this.variableStateManager = new VariableState();

    // Initialize the ConversationSession (without setting an initial message).
    // The initial message will be set asynchronously in the factory method.
    this.conversationManager = new ConversationSession({
      executionId: id,
      initialMessages: [],
      ...conversationManagerConfig,
    });
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
    // The ConversationSession does not provide a normalizeHistory method.
    // If this feature is needed, it can be added to the ConversationSession.
  }

  // Variable Management ============

  /**
   * Getting variables
   * @param name Variable name
   */
  getVariable(name: string): unknown {
    return this.variableStateManager.getVariableValue(name, "workflowExecution");
  }

  /**
   * Setting variables
   * @param name Variable name
   * @param value Variable value
   */
  setVariable(name: string, value: unknown): void {
    this.variableStateManager.setVariableValue(name, value, "workflowExecution");
  }

  /**
   * Get all variables
   */
  getAllVariables(): Record<string, unknown> {
    return this.variableStateManager.getVariablesByScope("workflowExecution");
  }

  /**
   * Deleting variables
   * @param name Variable name
   */
  deleteVariable(name: string): boolean {
    return this.variableStateManager.deleteVariable(name);
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
   * @param type Interrupt type
   */
  interrupt(type: "PAUSE" | "STOP"): void {
    this.state.interrupt(type);
    if (type === "STOP") {
      this.abort();
    }
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

    // Cleanup variable state manager
    this.variableStateManager.cleanup();

    // Clear abort controller
    this.abortController = undefined;

    // Clear queues
    this.clearAllQueues();
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

    // Create entity with restored state
    const entity = new AgentLoopEntity(id, snapshot.config as AgentLoopConfig, state);

    // Restore messages
    entity.setMessages(snapshot.messages as LLMMessage[]);

    // Restore variables
    for (const [key, value] of Object.entries(snapshot.variables)) {
      entity.setVariable(key, value);
    }

    return entity;
  }
}
